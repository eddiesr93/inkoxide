use std::fmt::Write as _;

use super::limits::MAX_DEPTH;
use super::*;
use crate::geom::Rect;

fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">{body}</svg>"#)
}

/// The error for `doc` (`unwrap_err` would print a huge tree).
fn rejected(doc: &str) -> String {
    match parse(doc.as_bytes()) {
        Ok(_) => panic!("accepted {}", &doc[doc.len().saturating_sub(120)..]),
        Err(e) => e,
    }
}

#[test]
fn parses_and_sizes() {
    let t = parse(svg(r#"<rect width="5" height="5" fill="red"/>"#).as_bytes())
        .unwrap()
        .0;
    assert_eq!((t.size().width(), t.size().height()), (20.0, 10.0));
    assert!(is_svg(b"\xEF\xBB\xBF  <?xml version=\"1.0\"?><svg/>"));
    assert!(!is_svg(b"\x89PNG"));
}

#[test]
fn refuses_sizes_and_view_boxes_too_small_to_scale() {
    let root = |attrs: &str| format!(r#"<svg xmlns="{SVG_NS}" {attrs}/>"#);
    for side in ["1e-6", "1e-12", "1e-20", "1e-30", "1e-39"] {
        for attrs in [
            format!(r#"width="{side}" height="10""#),
            format!(r#"width="10" height="{side}""#),
            format!(r#"viewBox="0 0 {side} 10""#),
            format!(r#"width="10" height="10" viewBox="0 0 10 {side}""#),
        ] {
            let e = rejected(&root(&attrs));
            assert!(e.contains("at least"), "{attrs}: {e}");
            // Nested, it is left out, as an invalid SVG is.
            let t = parse(image_of(&root(&attrs), 1, 0).as_bytes()).unwrap().0;
            assert_eq!(nested_images(t.root()), (0, 0), "{attrs}");
        }
    }
    // The smallest and largest sizes are accepted.
    let t = parse(root(r#"width="1e-5" height="100000""#).as_bytes())
        .unwrap()
        .0;
    assert_eq!((t.size().width(), t.size().height()), (MIN_SIDE, MAX_SIDE));
    parse(root(r#"width="100000" height="1" viewBox="0 0 1e-5 1e-5""#).as_bytes()).unwrap();
    let e = rejected(&root(r#"width="100001" height="10""#));
    assert!(e.contains("at most"), "{e}");
}

#[test]
fn reads_text_and_rejects_garbage() {
    // The tree leaves text out; it comes with what drawing it needs.
    let (_, _, text) = parse_text(svg("<text x='1' y='5'>Hi</text>").as_bytes()).unwrap();
    assert_eq!(text.unwrap().text.texts.len(), 1);
    // Empty text elements are no text.
    let (_, _, text) = parse_text(svg("<text x='1' y='5'> </text>").as_bytes()).unwrap();
    assert!(text.is_none());
    assert!(parse(b"<svg").is_err());
    assert!(parse(b"<html/>").is_err());
    assert!(parse(&[0xFF, 0xFE, 0x3C]).is_err());
    // Text in a nested SVG comes with the outer one.
    let outer = image_of(&svg("<text x='1' y='5'>Hi</text>"), 1, 0);
    let (_, _, text) = parse_text(outer.as_bytes()).unwrap();
    let text = text.unwrap();
    assert!(text.text.texts.is_empty());
    assert_eq!(text.nested[0].text.texts.len(), 1);
    // Text on a path and vertical text say so.
    let e = rejected(&svg(
        "<path id='p' d='M0 0H9'/><text><textPath href='#p'>Hi</textPath></text>",
    ));
    assert!(e.contains("<textPath>"), "{e}");
    let e = rejected(&svg("<text writing-mode='tb'>Hi</text>"));
    assert!(e.contains("vertical"), "{e}");
}

#[test]
fn counts_text_per_copy() {
    let text = "x".repeat(10_000);
    let copies = |n: usize| {
        svg(&format!(
            r#"<defs><text id="t">{text}</text></defs>{}"#,
            r##"<use href="#t"/>"##.repeat(n)
        ))
    };
    parse(copies(90).as_bytes()).unwrap();
    let e = rejected(&copies(110));
    assert!(e.contains("characters of text"), "{e}");
}

#[test]
fn an_embedded_raster_over_the_pixel_limit_fails_the_svg() {
    // A PNG header of 10000 × 10000 pixels: probing reads no further.
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend(10_000u32.to_be_bytes());
    png.extend(10_000u32.to_be_bytes());
    png.extend([8, 2, 0, 0, 0, 0, 0, 0, 0]);
    let body = format!(
        r#"<image width="4" height="4" href="{}"/>"#,
        url_of("image/png", &png)
    );
    let e = rejected(&svg(&body));
    assert!(e.contains("megapixels"), "{e}");
}

#[test]
fn rejects_reference_bombs() {
    // Ten levels of ten `use`s: 10^10 elements once expanded.
    let mut body = String::from(r#"<defs><rect id="l0" width="1" height="1"/>"#);
    for i in 1..=10 {
        let _ = write!(body, r#"<g id="l{i}">"#);
        for _ in 0..10 {
            let _ = write!(body, r##"<use href="#l{}"/>"##, i - 1);
        }
        body.push_str("</g>");
    }
    body.push_str(r##"</defs><use href="#l10"/>"##);
    let e = parse(svg(&body).as_bytes()).unwrap_err();
    assert!(e.contains("too complex"), "{e}");

    // Self-reference is not a bomb.
    parse(svg(r##"<g id="a"><use href="#a"/></g>"##).as_bytes()).unwrap();

    let mut deep = String::new();
    for _ in 0..(MAX_DEPTH + 5) {
        deep.push_str("<g>");
    }
    for _ in 0..(MAX_DEPTH + 5) {
        deep.push_str("</g>");
    }
    let e = parse(svg(&deep).as_bytes()).unwrap_err();
    assert!(e.contains("deeper"), "{e}");
}

#[test]
fn a_view_box_fits_the_box_it_fills_like_css() {
    let view_box = |attrs: &str| {
        let doc = format!(r#"<svg xmlns="{SVG_NS}" {attrs}/>"#);
        ViewBox::of(Document::parse(&doc).unwrap().root_element())
    };
    assert_eq!(view_box(r#"width="10" height="10""#), None);
    assert_eq!(view_box(r#"viewBox="0 0 0 10""#), None);
    let vb = view_box(r#"viewBox="0,0 100 100""#).unwrap();
    assert_eq!((vb.align, vb.slice), (Some((0.5, 0.5)), false));
    let bx = Rect::new(10.0, 20.0, 100.0, 40.0);
    let near = |(r, clip): (Rect, bool), want: [f32; 4], clipped: bool| {
        let got = [r.x, r.y, r.w, r.h];
        let close = got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-3);
        assert!(close && clip == clipped, "{got:?} {clip}, want {want:?}");
    };
    // A square drawing in a wide box: a 40 pt square, centred.
    near(
        vb.place((100.0, 100.0), bx),
        [40.0, 20.0, 40.0, 40.0],
        false,
    );
    let place = |attrs: &str, size| view_box(attrs).unwrap().place(size, bx);
    near(
        place(
            r#"viewBox="0 0 100 100" preserveAspectRatio="xMinYMax""#,
            (100.0, 100.0),
        ),
        [10.0, 20.0, 40.0, 40.0],
        false,
    );
    near(
        place(
            r#"viewBox="0 0 100 100" preserveAspectRatio="none""#,
            (100.0, 100.0),
        ),
        [10.0, 20.0, 100.0, 40.0],
        false,
    );
    // slice covers the box: 100 × 100, the middle 40 shown.
    near(
        place(
            r#"viewBox="0 0 100 100" preserveAspectRatio="defer xMidYMid slice""#,
            (100.0, 100.0),
        ),
        [10.0, -10.0, 100.0, 100.0],
        true,
    );
    // An SVG whose size is wider than its viewBox: the viewBox, not the
    // size, fits the box.
    near(
        place(r#"viewBox="0 0 100 100""#, (200.0, 100.0)),
        [20.0, 20.0, 80.0, 40.0],
        false,
    );
    // An invalid value is the default.
    near(
        place(
            r#"viewBox="0 0 100 100" preserveAspectRatio="xFooYMin""#,
            (100.0, 100.0),
        ),
        [40.0, 20.0, 40.0, 40.0],
        false,
    );
}

#[test]
fn ignores_files() {
    let t = parse(svg(r#"<image href="/etc/passwd" width="5" height="5"/>"#).as_bytes()).unwrap();
    assert!(t.0.root().children().is_empty());
}

/// `doc` as a percent-encoded `data:` URL, which nests without escaping.
fn data_url(doc: &str) -> String {
    url_of("image/svg+xml", doc.as_bytes())
}

/// `bytes` as a percent-encoded `data:` URL.
fn url_of(mime: &str, bytes: &[u8]) -> String {
    let mut url = format!("data:{mime},");
    for &b in bytes {
        if b.is_ascii_alphanumeric() {
            url.push(char::from(b));
        } else {
            let _ = write!(url, "%{b:02X}");
        }
    }
    url
}

/// An SVG drawing `copies` copies of `inner` as an `<image>`, inside
/// `groups` nested groups.
fn image_of(inner: &str, copies: usize, groups: usize) -> String {
    let mut body = format!(
        r#"<defs><image id="i" width="4" height="4" href="{}"/></defs>"#,
        data_url(inner)
    );
    body.push_str(&"<g>".repeat(groups));
    body.push_str(&r##"<use href="#i"/>"##.repeat(copies));
    body.push_str(&"</g>".repeat(groups));
    svg(&body)
}

/// `levels` of `<use>` bombs: about `10^levels` elements once expanded.
fn bomb(levels: usize) -> String {
    let mut body = String::from(r#"<defs><rect id="l0" width="1" height="1"/>"#);
    for i in 1..=levels {
        let _ = write!(body, r#"<g id="l{i}">"#);
        body.push_str(&format!(r##"<use href="#l{}"/>"##, i - 1).repeat(10));
        body.push_str("</g>");
    }
    let _ = write!(body, r##"</defs><use href="#l{levels}"/>"##);
    svg(&body)
}

/// Nested SVG images: (copies at the top level, deepest level).
fn nested_images(g: &usvg::Group) -> (usize, usize) {
    let (mut copies, mut deepest) = (0, 0);
    for n in g.children() {
        match n {
            usvg::Node::Group(g) => {
                let (c, d) = nested_images(g);
                copies += c;
                deepest = deepest.max(d);
            }
            usvg::Node::Image(img) => {
                if let usvg::ImageKind::SVG(t) = img.kind() {
                    copies += 1;
                    deepest = deepest.max(1 + nested_images(t.root()).1);
                }
            }
            _ => {}
        }
    }
    (copies, deepest)
}

#[test]
fn nested_svgs_share_the_budget() {
    // Each copy of a nested SVG is parsed again: a thousand copies of
    // 47k elements must not get a fresh budget each.
    let inner = bomb(4);
    crate::test_timing::assert_bounded("copies", 1000, |n| {
        let doc = image_of(&inner, n, 0);
        let t = std::time::Instant::now();
        let e = rejected(&doc);
        let took = t.elapsed();
        assert!(e.contains("too complex"), "{e}");
        took
    });

    // Nor each level: 50 copies of 50 copies of 4.7k elements.
    let mid = image_of(&bomb(3), 50, 0);
    let e = parse(image_of(&mid, 50, 0).as_bytes()).unwrap_err();
    assert!(e.contains("too complex"), "{e}");

    // A few copies fit.
    let t = parse(image_of(&inner, 3, 0).as_bytes()).unwrap().0;
    assert_eq!(nested_images(t.root()), (3, 1));
}

#[test]
fn caps_svg_in_svg_nesting() {
    let chain = |levels: usize, groups: usize| {
        let mut doc = svg(r#"<rect width="5" height="5"/>"#);
        for _ in 0..levels {
            doc = image_of(&doc, 1, groups);
        }
        doc
    };
    let e = parse(chain(MAX_NESTING + 1, 0).as_bytes()).unwrap_err();
    assert!(e.contains("too complex") && e.contains("nest"), "{e}");

    // Element depth adds up across levels: each level is its groups,
    // root, `use` and image deep, and the innermost has a rect.
    let groups = (MAX_DEPTH - 1) / MAX_NESTING - 3;
    let e = parse(chain(MAX_NESTING, groups + 1).as_bytes()).unwrap_err();
    assert!(e.contains("deeper"), "{e}");

    // The deepest accepted chain converts on a small stack.
    let doc = chain(MAX_NESTING, groups);
    let t = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || parse(doc.as_bytes()).unwrap().0)
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(nested_images(t.root()).1, MAX_NESTING);
}

#[test]
fn limits_embedded_image_bytes() {
    // `usvg` decodes the URL again for every copy.
    let png = format!("data:image/png;base64,{}", "A".repeat(1 << 20));
    let body = format!(r#"<defs><image id="i" width="4" height="4" href="{png}"/></defs>"#);
    let e =
        parse(svg(&(body.clone() + &r##"<use href="#i"/>"##.repeat(200))).as_bytes()).unwrap_err();
    assert!(e.contains("MiB"), "{e}");
    parse(svg(&(body + &r##"<use href="#i"/>"##.repeat(2))).as_bytes()).unwrap();
}

#[test]
fn counts_markers_per_vertex() {
    let marker = |content: &str| format!(r#"<defs><marker id="m">{content}</marker></defs>"#);
    let dot = marker(r#"<circle r="1"/>"#);
    let long = format!("M0 0{}", "h1".repeat(200_000));
    // A dot at each of 200k vertices, however the marker is set, is
    // refused at the cost of the path alone.
    for shape in [
        format!(r#"<path d="{long}" marker-mid="url(#m)"/>"#),
        format!(r#"<path d="{long}" style="MARKER-MID: url(#m)"/>"#),
        format!(r#"<path d="{long}" style="/**/marker: url(#m)"/>"#),
        format!(r#"<g marker-mid="url(#m)"><path d="{long}"/></g>"#),
        format!(r#"<style>path {{ marker-mid: url(#m) }}</style><path d="{long}"/>"#),
        format!(r##"<path id="p" d="{long}"/><use href="#p" marker-mid="url(#m)"/>"##),
    ] {
        let plain = svg(&format!(r#"<path d="{long}"/>"#));
        let marked = svg(&(dot.clone() + &shape));
        crate::test_timing::assert_as_fast(
            &format!("{shape:.60}"),
            || {
                let t = std::time::Instant::now();
                parse(plain.as_bytes()).unwrap();
                t.elapsed()
            },
            || {
                let t = std::time::Instant::now();
                let e = rejected(&marked);
                let took = t.elapsed();
                assert!(e.contains("too complex"), "{shape:.60}: {e}");
                took
            },
        );
    }
    // An embedded image in a marker is decoded at every vertex.
    let png = format!("data:image/png;base64,{}", "A".repeat(1 << 20));
    let image = marker(&format!(r#"<image width="1" height="1" href="{png}"/>"#));
    let path = format!(
        r#"<path d="M0 0{}" marker-mid="url(#m)"/>"#,
        "h1".repeat(200)
    );
    let e = rejected(&svg(&(image + &path)));
    assert!(e.contains("MiB"), "{e}");

    // Arrowheads at the ends of a long path, dots along a short line.
    let arrows = format!(r#"<path d="{long}" marker-start="url(#m)" marker-end="url(#m)"/>"#);
    parse(svg(&(dot.clone() + &arrows)).as_bytes()).unwrap();
    let dots = format!(
        r#"<polyline points="{}" marker-mid="url(#m)"/>"#,
        "1,1 ".repeat(100)
    );
    parse(svg(&(dot + &dots)).as_bytes()).unwrap();
}

/// A PNG header declaring `w`×`h` pixels, made distinct by `salt`.
fn png_url(w: u32, h: u32, salt: u8) -> String {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend(w.to_be_bytes());
    png.extend(h.to_be_bytes());
    png.extend([8, 2, 0, 0, 0, salt, 0, 0, 0]);
    url_of("image/png", &png)
}

#[test]
fn limits_embedded_pixels() {
    let image = |salt| {
        let url = png_url(7000, 7000, salt);
        format!(r#"<image id="i{salt}" width="1" height="1" href="{url}"/>"#)
    };
    // An image is decoded once, however many copies draw it.
    let copies = r##"<use href="#i0"/>"##.repeat(1000);
    parse(svg(&format!("<defs>{}</defs>{copies}", image(0))).as_bytes()).unwrap();
    // Five 49-megapixel images are too many.
    let five: String = (0..5).map(image).collect();
    let e = rejected(&svg(&five));
    assert!(e.contains("megapixels"), "{e}");
}

#[test]
fn limits_path_data() {
    // `usvg` parses path data again for every copy.
    let d = format!("M0 0{}", "h1".repeat(1 << 17));
    let body = format!(r#"<defs><path id="p" d="{d}"/></defs>"#);
    let e = rejected(&svg(&(body.clone() + &r##"<use href="#p"/>"##.repeat(130))));
    assert!(e.contains("path data"), "{e}");
    parse(svg(&(body + &r##"<use href="#p"/>"##.repeat(2))).as_bytes()).unwrap();
}

#[test]
fn charges_shared_paint_servers_once() {
    let pattern = |href: &str| {
        format!(
            r#"<defs><pattern id="p" width="1" height="1"><image width="1" height="1" href="{href}"/></pattern></defs>"#
        )
    };
    let rects = |n| r#"<rect width="5" height="5" fill="url(#p)"/>"#.repeat(n);
    // `usvg` converts a pattern once: its image is decoded once.
    let mut png = crate::test_fixtures::fixture("images/logo.png");
    png.resize(1 << 20, 0);
    let png = url_of("image/png", &png);
    parse(svg(&(pattern(&png) + &rects(200))).as_bytes()).unwrap();
    // Its content is still copied for every shape.
    let e = rejected(&svg(&(pattern(&data_url(&bomb(4))) + &rects(10))));
    assert!(e.contains("too complex"), "{e}");
}

#[test]
fn limits_styles() {
    type Doc<'a> = &'a dyn Fn(usize) -> String;
    // The CSS parser scans a style from its start for every
    // declaration, and `usvg` parses a copied element's style again:
    // what is too long is refused in linear time. It also scans after
    // every unitless number, in sheets too.
    let style = "fill:none;".repeat(100);
    let copied = |copies: usize| {
        let uses = r##"<use href="#a"/>"##.repeat(copies);
        svg(&format!(
            r#"<defs><g id="a" style="{style}"/></defs>{uses}"#
        ))
    };
    let dashes = |n: usize| format!("stroke-dasharray:{}", "1 ".repeat(n));
    let declarations = |n: usize| svg(&format!(r#"<g style="{}"/>"#, "fill:none;".repeat(n)));
    let numbers = |n: usize| svg(&format!(r#"<rect style="{}"/>"#, dashes(n)));
    let in_sheet = |n: usize| svg(&format!("<style>rect{{{}}}</style><rect/>", dashes(n)));
    let docs: [(&str, usize, Doc); 4] = [
        ("declarations", 25_000, &declarations),
        ("numbers", 20_000, &numbers),
        ("numbers in a sheet", 20_000, &in_sheet),
        ("copies", 7_500, &copied),
    ];
    for (what, n, doc) in docs {
        crate::test_timing::assert_linear(what, n, |n| {
            let doc = doc(n);
            let t = std::time::Instant::now();
            let e = rejected(&doc);
            let took = t.elapsed();
            assert!(e.contains("too long"), "{what}: {e}");
            took
        });
    }
    parse(svg(&format!(r#"<rect style="{}"/>"#, dashes(1_000))).as_bytes()).unwrap();
    parse(copied(100).as_bytes()).unwrap();

    // Every style sheet rule is matched against each copy.
    let sheet = format!("<style>{}</style>", ".a{}".repeat(2_000));
    let uses = r##"<use href="#r"/>"##.repeat(30_000);
    let e = rejected(&svg(&format!(
        r#"{sheet}<defs><rect id="r"/></defs>{uses}"#
    )));
    assert!(e.contains("rule matches"), "{e}");
    parse(svg(&format!("{sheet}<rect/>")).as_bytes()).unwrap();

    // Declarations are copied onto every element they match.
    let sheet = format!("<style>{}</style>", ".a{fill:red}".repeat(2_000));
    let e = rejected(&svg(&format!(
        r#"{sheet}<defs><rect id="r"/></defs>{uses}"#
    )));
    assert!(e.contains("copy more than"), "{e}");
}

#[test]
fn weighs_selectors_by_their_combinators() {
    // Each descendant combinator may try every ancestor.
    let groups = 60;
    let deep = format!("{}<rect/>{}", "<g>".repeat(groups), "</g>".repeat(groups));
    let selector = vec!["*"; 30].join(" ");
    // A selector list is one rule per selector.
    let list = vec!["g rect"; 20_000].join(",");
    let flat = "<rect/>".repeat(3_000);
    // Ordinary sheets fit, and the others are refused as fast.
    let sheet = "<style>.st0{fill:#fff} svg g .st1{stroke:red} #a > path{fill:none}</style>";
    let ordinary = svg(&format!("{sheet}{deep}{flat}"));
    for (what, doc) in [
        (
            "combinators",
            svg(&format!("<style>{selector} {{}}</style>{deep}")),
        ),
        (
            "a selector list",
            svg(&format!("<style>{list} {{}}</style>{flat}")),
        ),
    ] {
        crate::test_timing::assert_as_fast(
            what,
            || {
                let t = std::time::Instant::now();
                parse(ordinary.as_bytes()).unwrap();
                t.elapsed()
            },
            || {
                let t = std::time::Instant::now();
                let e = rejected(&doc);
                let took = t.elapsed();
                assert!(e.contains("rule matches"), "{what}: {e}");
                took
            },
        );
    }
}

#[test]
fn refuses_huge_radii() {
    for shape in [
        r#"<path d="M0 0A1e60 1e60 0 1 1 1e60 0"/>"#,
        r#"<path d="M0 0a1e30,1e30 0 1,1 1,0"/>"#,
        r#"<circle r="3e38"/>"#,
        r#"<ellipse rx="1" ry="1e300"/>"#,
        r#"<rect width="5" height="5" rx="1e9"/>"#,
        r#"<circle style="r: 1e20"/>"#,
    ] {
        let ordinary = svg(r#"<path d="M0 0A5 5 0 1 1 10 0"/><circle r="4"/>"#);
        crate::test_timing::assert_as_fast(
            shape,
            || {
                let t = std::time::Instant::now();
                parse(ordinary.as_bytes()).unwrap();
                t.elapsed()
            },
            || {
                let t = std::time::Instant::now();
                let e = rejected(&svg(shape));
                let took = t.elapsed();
                assert!(e.contains("larger than"), "{shape}: {e}");
                took
            },
        );
    }
}

#[test]
fn counts_markers_on_every_shape() {
    let marker = r#"<defs><marker id="m"><path d="M0 0h1"/></marker></defs>"#;
    let shapes = r#"<circle r="2" marker-mid="url(#m)"/>"#.repeat(2_000);
    let e = rejected(&svg(&format!("{marker}{shapes}")));
    assert!(e.contains("too complex"), "{e}");
    // Arcs in path data become several curves, each with a vertex.
    let arcs = format!(
        r#"<path d="M0 0{}" marker-mid="url(#m)"/>"#,
        "A1 1 0 0 1 1 1".repeat(3_000)
    );
    let e = rejected(&svg(&format!("{marker}{arcs}")));
    assert!(e.contains("too complex"), "{e}");
    let few = r#"<circle r="2" marker-mid="url(#m)"/>"#.repeat(10);
    parse(svg(&format!("{marker}{few}")).as_bytes()).unwrap();
}

#[test]
fn follows_links_as_usvg_reads_them() {
    // Spaces around a link, quotes in `url()`, and a plain `href` that
    // wins over `xlink:href` all reach the referenced element.
    let bomb = bomb(5);
    let body = &bomb[bomb.find("<defs>").unwrap()..bomb.rfind("<use").unwrap()];
    for user in [
        r#"<use href=" #l5 "/>"#,
        r##"<use xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="#l0" href="#l5"/>"##,
        r#"<rect width="1" height="1" fill="url( '#p ' )"/>"#,
    ] {
        let pattern = r##"<pattern id="p" width="1" height="1"><use href="#l5"/></pattern>"##;
        let doc = svg(&format!("{body}<defs>{pattern}</defs>{}", user.repeat(3)));
        let e = rejected(&doc);
        assert!(e.contains("too complex"), "{user}: {e}");
    }
    // The last of two elements with one id is followed too.
    let doc = svg(&format!(
        r##"{body}<defs><marker id="m"/><marker id="m"><use href="#l5"/></marker></defs><path d="M0 0h1h1" marker-mid="url(#m)"/>"##
    ));
    let e = rejected(&doc);
    assert!(e.contains("too complex"), "{e}");
}

#[test]
fn refuses_href_loops() {
    let e = rejected(&svg(
        r##"<defs><linearGradient id="a" href="#b"/><linearGradient id="b" href="#c"/><linearGradient id="c" href="#b"/></defs><rect width="5" height="5" fill="url(#a)"/>"##,
    ));
    assert!(e.contains("loop"), "{e}");
    // A chain that ends, or an element naming itself, is fine.
    parse(
        svg(r##"<defs><linearGradient id="a" href="#b"/><linearGradient id="b" href="#b"><stop offset="0"/></linearGradient></defs><rect width="5" height="5" fill="url(#a)"/>"##)
            .as_bytes(),
    )
    .unwrap();
}

#[test]
fn limits_entities() {
    let entity = |value: &str, uses: usize| {
        format!(
            r#"<!DOCTYPE svg [<!ENTITY e "{value}">]><svg xmlns="http://www.w3.org/2000/svg" width="5" height="5"><g class="{}"/></svg>"#,
            "&e;".repeat(uses)
        )
    };
    let e = rejected(&entity(&"x".repeat(1024), 2_000));
    assert!(e.contains("entities expand"), "{e}");
    let e = rejected(&entity("&amp;&amp;", 1));
    assert!(e.contains("other entities"), "{e}");
    // The namespace entities of drawing programs.
    let illustrator = r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [
        <!ENTITY ns_svg "http://www.w3.org/2000/svg">
        <!ENTITY ns_xlink "http://www.w3.org/1999/xlink">
    ]><svg xmlns="&ns_svg;" xmlns:xlink="&ns_xlink;" width="5" height="5"><rect width="5" height="5"/></svg>"#;
    parse(illustrator.as_bytes()).unwrap();
}

#[test]
fn charges_inherited_attributes_per_shape() {
    // An inherited attribute is stored again on every shape.
    let dashes = "1 ".repeat(100_000);
    let rects = r#"<rect width="1" height="1"/>"#.repeat(300);
    let e = rejected(&svg(&format!(
        r#"<g stroke="red" stroke-dasharray="{dashes}">{rects}</g>"#
    )));
    assert!(e.contains("attributes"), "{e}");
    parse(
        svg(&format!(
            r#"<g stroke="red" stroke-dasharray="1 2">{rects}</g>"#
        ))
        .as_bytes(),
    )
    .unwrap();
}

#[test]
fn charges_inherited_text_properties_per_span() {
    // Each span of a text reads the properties it inherits again.
    let families = vec!["a"; 50_000].join(",");
    let spans = "<tspan>a</tspan>".repeat(400);
    let e = rejected(&svg(&format!(
        r#"<g font-family="{families}"><text>{spans}</text></g>"#
    )));
    assert!(e.contains("attributes"), "{e}");
    parse(
        svg(&format!(
            r#"<g font-family="Inter"><text>{spans}</text></g>"#
        ))
        .as_bytes(),
    )
    .unwrap();
}
