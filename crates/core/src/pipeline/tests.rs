//! Headers and footers with page numbers, through layout, pagination and
//! paint.

use super::*;
use crate::images::{ImageCrateDecoder, ImageStore};
use crate::FontStore;

/// Page size; the flow is one box sliced over the pages.
const W: f32 = 200.0;
const H: f32 = 100.0;
/// Horizontal padding of the footer.
const PAD: f32 = 10.0;

fn store() -> FontStore {
    let mut s = FontStore::new();
    for name in ["Inter-Regular", "NotoSansHebrew-Regular"] {
        let path = format!(
            "{}/../../fixtures/fonts/{name}.ttf",
            env!("CARGO_MANIFEST_DIR")
        );
        s.register(name, std::fs::read(path).unwrap()).unwrap();
    }
    s
}

/// A document of `pages` pages whose footer is `footer` (a node).
fn doc(pages: u32, footer: &str) -> String {
    // A box a little shorter than `pages` pages takes exactly `pages`.
    let flow = f32::from(u16::try_from(pages).unwrap()) * H - 1.0;
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}},
            {{"id":"NotoSansHebrew-Regular","family":"Noto Sans Hebrew"}}],
            "fallbackFonts":["Inter-Regular"],
            "pages":[{{"size":{{"width":{W},"height":{H}}},"footer":{footer},
            "children":[{{"type":"box","style":{{"height":{flow}}}}}]}}]}}"#
    )
}

/// "Page N of M" in a text node with `style`.
fn page_n_of_m(style: &str) -> String {
    n_of_m(style, "Inter-Regular", "", ["Page ", " of "], None)
}

/// `words[0]` N `words[1]` M in a text node with `style`, in `font` with
/// the run properties `extra`: placeholders, or the `written` numbers.
fn n_of_m(
    style: &str,
    font: &str,
    extra: &str,
    words: [&str; 2],
    written: Option<(u32, u32)>,
) -> String {
    let run = |r: &str| format!(r#"{{{r},"font":"{font}","size":10{extra}}}"#);
    let number = |placeholder: &str, n: Option<u32>| match n {
        Some(n) => run(&format!(r#""text":"{n}""#)),
        None => run(&format!(r#""placeholder":"{placeholder}""#)),
    };
    format!(
        r#"{{"type":"text","style":{{{style}}},"runs":[{},{},{},{}]}}"#,
        run(&format!(r#""text":"{}""#, words[0])),
        number("pageNumber", written.map(|w| w.0)),
        run(&format!(r#""text":"{}""#, words[1])),
        number("totalPages", written.map(|w| w.1)),
    )
}

/// A footer box with `style` (and the horizontal padding) around `child`.
fn footer(style: &str, child: &str) -> String {
    format!(
        r#"{{"type":"box","style":{{{style}"padding":{{"left":{PAD},"right":{PAD},"bottom":10}}}},
            "children":[{child}]}}"#
    )
}

/// A glyph painted on a page: baseline, horizontal extent, and the digit
/// it shows, if any.
struct Painted {
    y: f32,
    x0: f32,
    x1: f32,
    digit: Option<char>,
}

/// The glyphs painted on every page.
fn painted(json: &str) -> Vec<Vec<Painted>> {
    let store = store();
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts), None);
    let images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    let (pages, _) = run(&doc, &fonts, &mut text, &images, true, Ok).unwrap();
    pages
        .iter()
        .map(|page| {
            let mut out = Vec::new();
            for op in &page.ops {
                let Op::Text(t) = op else { continue };
                let font = fonts.get(t.font);
                for g in &t.glyphs {
                    let x0 = t.x + g.x;
                    out.push(Painted {
                        y: t.y,
                        x0,
                        x1: x0 + font.advance(g.id) * font.scale(t.size),
                        digit: ('0'..='9').find(|&c| font.glyph(c) == Some(g.id)),
                    });
                }
            }
            out
        })
        .collect()
}

/// The painted lines of every page: baseline and horizontal extent of the
/// glyphs on it, top to bottom.
fn painted_lines(json: &str) -> Vec<Vec<(f32, f32, f32)>> {
    painted(json)
        .iter()
        .map(|glyphs| {
            let mut lines: Vec<(f32, f32, f32)> = Vec::new();
            for g in glyphs {
                match lines.iter_mut().find(|l| (l.0 - g.y).abs() < 0.01) {
                    Some(l) => *l = (l.0, l.1.min(g.x0), l.2.max(g.x1)),
                    None => lines.push((g.y, g.x0, g.x1)),
                }
            }
            lines.sort_by(|a, b| a.0.total_cmp(&b.0));
            lines
        })
        .collect()
}

/// The numbers painted on every page, as they read from left to right.
fn painted_numbers(json: &str) -> Vec<Vec<String>> {
    painted(json)
        .into_iter()
        .map(|mut glyphs| {
            glyphs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            let mut numbers: Vec<String> = Vec::new();
            let mut in_number = false;
            for g in glyphs {
                match (g.digit, in_number) {
                    (Some(d), true) => numbers.last_mut().unwrap().push(d),
                    (Some(d), false) => numbers.push(d.to_string()),
                    (None, _) => {}
                }
                in_number = g.digit.is_some();
            }
            numbers
        })
        .collect()
}

/// Page counts with one and with four digits.
const COUNTS: [u32; 2] = [7, 1234];

/// Asserts `f` of every painted line of every page, with the page number.
fn each_line(json: &str, pages: u32, f: impl Fn(u32, &[(f32, f32, f32)])) {
    let painted = painted_lines(json);
    assert_eq!(painted.len(), pages as usize);
    for (k, lines) in painted.iter().enumerate() {
        assert!(!lines.is_empty(), "page {}: nothing painted", k + 1);
        f(k as u32 + 1, lines);
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn right_aligned_page_numbers_end_at_the_edge() {
    let right = W - PAD;
    for pages in COUNTS {
        // Pushed right by the footer: the text box is as wide as the text.
        let pushed = footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &page_n_of_m(""),
        );
        // Aligned right inside a full-width text box.
        let aligned = footer("", &page_n_of_m(r#""textAlign":"right""#));
        for f in [pushed, aligned] {
            each_line(&doc(pages, &f), pages, |n, lines| {
                assert_eq!(lines.len(), 1, "page {n} of {pages}: {lines:?}");
                let (_, _, x1) = lines[0];
                assert!(close(x1, right), "page {n} of {pages} ends at {x1}");
            });
        }
    }
}

#[test]
fn centred_page_numbers_are_centred() {
    for pages in COUNTS {
        let pushed = footer(
            r#""flexDirection":"row","justifyContent":"center","#,
            &page_n_of_m(""),
        );
        let aligned = footer("", &page_n_of_m(r#""textAlign":"center""#));
        for f in [pushed, aligned] {
            each_line(&doc(pages, &f), pages, |n, lines| {
                assert_eq!(lines.len(), 1, "page {n} of {pages}: {lines:?}");
                let (_, x0, x1) = lines[0];
                let mid = f32::midpoint(x0, x1);
                assert!(close(mid, W / 2.0), "page {n} of {pages} centred at {mid}");
            });
        }
    }
}

/// In a 60 pt box, "Page 9 of 9" fits on one line where the stand-in
/// "Page 999 of 999" would not, and "Page 1000 of" no longer fits where
/// "Page 999 of" would.
#[test]
fn page_numbers_wrap_at_their_own_width() {
    let width = 60.0;
    let right = W - PAD;
    let f = footer(
        r#""flexDirection":"row","justifyContent":"flex-end","#,
        &page_n_of_m(&format!(r#""width":{width},"textAlign":"right""#)),
    );
    each_line(&doc(9, &f), 9, |n, lines| {
        assert_eq!(lines.len(), 1, "page {n} of 9: {lines:?}");
        assert!(close(lines[0].2, right), "page {n}: {lines:?}");
    });
    let pages = 1000;
    each_line(&doc(pages, &f), pages, |n, lines| {
        assert_eq!(lines.len(), 2, "page {n} of {pages}: {lines:?}");
        for &(_, x0, x1) in lines {
            assert!(close(x1, right), "page {n}: {lines:?}");
            assert!(x0 >= right - width - 0.01, "page {n} overflows: {lines:?}");
        }
    });
}

/// The extent of the one line of `footer` on its first page.
fn extent(footer: &str) -> (f32, f32) {
    let lines = &painted_lines(&doc(1, footer))[0];
    assert_eq!(lines.len(), 1, "{lines:?}");
    (lines[0].1, lines[0].2)
}

/// Asserts that `footer(None)` with page numbers lays out on each of
/// `pages` pages like `footer(Some((n, pages)))` with them written out.
fn like_written(pages: u32, footer: &dyn Fn(Option<(u32, u32)>) -> String) {
    each_line(&doc(pages, &footer(None)), pages, |n, lines| {
        assert_eq!(lines.len(), 1, "page {n} of {pages}: {lines:?}");
        let (x0, x1) = extent(&footer(Some((n, pages))));
        assert!(
            close(lines[0].1, x0) && close(lines[0].2, x1),
            "page {n} of {pages}: {lines:?}, written out {x0}..{x1}"
        );
    });
}

/// "עמוד N מתוך M" ("page N of M") in a right-to-left paragraph, on pages
/// whose numbers take one and two digits.
#[test]
fn right_to_left_page_numbers_read_left_to_right() {
    let pages = 12;
    let right = W - PAD;
    let hebrew = |style: &str, written| {
        n_of_m(
            &format!(r#""direction":"rtl"{style}"#),
            "NotoSansHebrew-Regular",
            "",
            [
                "\u{05e2}\u{05de}\u{05d5}\u{05d3} ",
                " \u{05de}\u{05ea}\u{05d5}\u{05da} ",
            ],
            written,
        )
    };
    // At the start of the line, which is its right end; pushed right by
    // the footer, in a box as wide as the text; and centred.
    let aligned = |w| footer("", &hebrew("", w));
    let pushed = |w| {
        footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &hebrew("", w),
        )
    };
    let centred = |w| footer("", &hebrew(r#","textAlign":"center""#, w));
    like_written(pages, &aligned);
    like_written(pages, &pushed);
    like_written(pages, &centred);
    for f in [aligned(None), pushed(None)] {
        each_line(&doc(pages, &f), pages, |n, lines| {
            assert!(close(lines[0].2, right), "page {n}: {lines:?}");
        });
        // The total is on the left, and each number reads left to right
        // ("10", not "01").
        for (k, numbers) in painted_numbers(&doc(pages, &f)).iter().enumerate() {
            assert_eq!(numbers, &["12".to_string(), (k + 1).to_string()]);
        }
    }
}

#[test]
fn letter_spaced_page_numbers_take_their_own_width() {
    let pages = 12;
    let spaced = |style: &str, written| {
        n_of_m(
            style,
            "Inter-Regular",
            r#","letterSpacing":3"#,
            ["Page ", " of "],
            written,
        )
    };
    let aligned = |w| footer("", &spaced(r#""textAlign":"right""#, w));
    let pushed = |w| {
        footer(
            r#""flexDirection":"row","justifyContent":"flex-end","#,
            &spaced("", w),
        )
    };
    like_written(pages, &aligned);
    like_written(pages, &pushed);
    for f in [aligned(None), pushed(None)] {
        let ends: Vec<f32> = painted_lines(&doc(pages, &f))
            .iter()
            .map(|lines| lines[0].2)
            .collect();
        assert!(ends.iter().all(|&x| close(x, ends[0])), "{ends:?}");
        for (k, numbers) in painted_numbers(&doc(pages, &f)).iter().enumerate() {
            assert_eq!(numbers, &[(k + 1).to_string(), pages.to_string()]);
        }
    }
}

// Fixed nodes: overlays (fixed and absolutely positioned, at the top of the
// flow) and fixed nodes in the flow.

/// Page margin in the documents with fixed nodes; the content box is
/// `W - 2·M` by `H - 2·M`.
const M: f32 = 10.0;

/// A document of `pages` pages (margins `M`) whose flow is `fixed` (one or
/// more nodes) followed by a box that fills the pages.
fn with_fixed(pages: u32, fixed: &str) -> String {
    let flow = f32::from(u16::try_from(pages).unwrap()) * (H - 2.0 * M) - 1.0;
    format!(
        r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
            "pages":[{{"size":{{"width":{W},"height":{H}}},
            "margin":{{"top":{M},"right":{M},"bottom":{M},"left":{M}}},
            "children":[{fixed},{{"type":"box","style":{{"height":{flow}}}}}]}}]}}"#
    )
}

/// A red box with the node properties `extra` and the style `style`.
fn red_box(extra: &str, style: &str) -> String {
    format!(r#"{{"type":"box"{extra},"style":{{"backgroundColor":[1,0,0,1],{style}}}}}"#)
}

/// Runs `f` on the painted pages and the marks of a document.
fn render_doc<R>(json: &str, f: impl FnOnce(&[PageContent], &Marks<'_>) -> R) -> R {
    let store = store();
    let doc = crate::validate::parse(json.as_bytes(), crate::InputFormat::Json).unwrap();
    let fonts = FontSet::load(&doc, &store).unwrap();
    let mut text = TextCache::new(RustybuzzShaper::new(&fonts), None);
    let images = ImageSet::load(&doc.pages, &ImageStore::new(), &ImageCrateDecoder).unwrap();
    let (pages, marks) = run(&doc, &fonts, &mut text, &images, true, Ok).unwrap();
    f(&pages, &marks)
}

type Bounds = (f32, f32, f32, f32);

/// The red rectangles painted on every page, as `(x, y, w, h)`.
fn red_boxes(json: &str) -> Vec<Vec<Bounds>> {
    render_doc(json, |pages, _| {
        pages
            .iter()
            .map(|page| {
                let mut out = Vec::new();
                for op in &page.ops {
                    let Op::FillPath { path, color, .. } = op else {
                        continue;
                    };
                    if *color != [1.0, 0.0, 0.0, 1.0] {
                        continue;
                    }
                    let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
                    let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                    for seg in &path.segs {
                        if let paint::PathSeg::MoveTo(x, y) | paint::PathSeg::LineTo(x, y) = *seg {
                            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                        }
                    }
                    out.push((x0, y0, x1 - x0, y1 - y0));
                }
                out
            })
            .collect()
    })
}

fn close_bounds(a: Bounds, b: Bounds) -> bool {
    close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3)
}

/// Asserts that the document has `pages` pages and each shows one red box
/// at `want`.
fn on_every_page(json: &str, pages: u32, want: Bounds) {
    let boxes = red_boxes(json);
    assert_eq!(boxes.len(), pages as usize, "{boxes:?}");
    for (k, b) in boxes.iter().enumerate() {
        assert!(
            b.len() == 1 && close_bounds(b[0], want),
            "page {}: {b:?}, want {want:?}",
            k + 1
        );
    }
}

const FIXED: &str = r#","break":{"fixed":true}"#;

#[test]
fn overlays_are_placed_in_the_content_box_of_every_page() {
    let (cw, ch) = (W - 2.0 * M, H - 2.0 * M);
    // From the bottom-right corner of one page's content box, not of the
    // flow (several pages tall).
    let corner = red_box(
        FIXED,
        r#""position":"absolute","inset":{"bottom":5,"right":5},"width":20,"height":10"#,
    );
    on_every_page(
        &with_fixed(3, &corner),
        3,
        (M + cw - 25.0, M + ch - 15.0, 20.0, 10.0),
    );
    // Percentages are of the content box.
    let centre = red_box(
        FIXED,
        r#""position":"absolute","inset":{"top":"50%","left":"25%"},"width":"50%","height":"10%""#,
    );
    on_every_page(
        &with_fixed(2, &centre),
        2,
        (M + cw / 4.0, M + ch / 2.0, cw / 2.0, ch / 10.0),
    );
    // Insets on all sides size the box; negative ones reach into the margin.
    let border = red_box(
        FIXED,
        r#""position":"absolute","inset":{"top":-5,"right":-5,"bottom":-5,"left":-5}"#,
    );
    on_every_page(
        &with_fixed(4, &border),
        4,
        (M - 5.0, M - 5.0, cw + 10.0, ch + 10.0),
    );
}

#[test]
fn overlays_take_no_space() {
    // Below the content box: painted off the page, without adding pages.
    let low = red_box(
        FIXED,
        r#""position":"absolute","inset":{"top":1000},"width":20,"height":10"#,
    );
    assert_eq!(red_boxes(&with_fixed(2, &low)).len(), 2);
    // The flow starts at the top of the content box, as without it.
    let flow = with_fixed(
        1,
        &format!(
            "{},{}",
            red_box(FIXED, r#""position":"absolute","width":20,"height":10"#),
            red_box("", r#""height":5"#)
        ),
    );
    let boxes = &red_boxes(&flow)[0];
    assert_eq!(boxes.len(), 2, "{boxes:?}");
    assert!(close(boxes[0].1, M) && close(boxes[1].1, M), "{boxes:?}");
}

#[test]
fn overlays_show_the_page_they_are_on() {
    let pages = 12;
    let numbers = n_of_m(
        r#""position":"absolute","inset":{"bottom":0,"right":0}"#,
        "Inter-Regular",
        "",
        ["Page ", " of "],
        None,
    );
    let overlay = numbers.replacen(r#""type":"text""#, &format!(r#""type":"text"{FIXED}"#), 1);
    let json = with_fixed(pages, &overlay);
    for (k, numbers) in painted_numbers(&json).iter().enumerate() {
        assert_eq!(numbers, &[(k + 1).to_string(), pages.to_string()]);
    }
    // Laid out with each page's digits: it ends at the content box's edge.
    each_line(&json, pages, |n, lines| {
        assert_eq!(lines.len(), 1, "page {n}: {lines:?}");
        assert!(close(lines[0].2, W - M), "page {n}: {lines:?}");
    });
}

#[test]
fn overlays_follow_their_page_filter() {
    let overlay = red_box(
        &format!(r#"{FIXED},"pages":"notFirst""#),
        r#""position":"absolute","width":20,"height":10"#,
    );
    let counts: Vec<usize> = red_boxes(&with_fixed(3, &overlay))
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(counts, [0, 1, 1]);
}

#[test]
fn fixed_nodes_in_the_flow_repeat_where_they_are() {
    // Not positioned: it takes its room once, in the flow (which then needs
    // a fourth page), and is painted at the same place on every page.
    let fixed = red_box(FIXED, r#""height":10"#);
    on_every_page(&with_fixed(3, &fixed), 4, (M, M, W - 2.0 * M, 10.0));
}

/// The colours of the shapes filled on each page, in paint order.
fn fills(json: &str) -> Vec<Vec<f32>> {
    render_doc(json, |pages, _| {
        pages
            .iter()
            .map(|page| {
                page.ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::FillPath { color, .. } => Some(color[0]),
                        _ => None,
                    })
                    .collect()
            })
            .collect()
    })
}

#[test]
fn overlays_with_a_negative_z_index_are_painted_under_the_flow() {
    // Boxes told apart by their red component.
    let shade = |red: f32, extra: &str, style: &str| {
        format!(r#"{{"type":"box"{extra},"style":{{"backgroundColor":[{red},0,0,1],{style}}}}}"#)
    };
    let overlay = |red: f32, z: &str| {
        shade(
            red,
            FIXED,
            &format!(r#""position":"absolute","width":20,"height":10{z}"#),
        )
    };
    let children = [
        overlay(0.1, r#","zIndex":-1"#),
        overlay(0.2, r#","zIndex":2"#),
        shade(0.5, "", r#""height":10"#),
        overlay(0.3, ""),
        overlay(0.4, r#","zIndex":-2"#),
        overlay(0.6, r#","zIndex":-1"#),
    ];
    let pages = fills(&with_fixed(1, &children.join(",")));
    // By `zIndex` then document order: the negative ones before the flow
    // (0.5, on the first page), the others after it.
    assert_eq!(
        pages,
        [
            vec![0.4, 0.1, 0.6, 0.5, 0.3, 0.2],
            vec![0.4, 0.1, 0.6, 0.3, 0.2]
        ]
    );
    // Under the flow, an overlay still shows the page it is on.
    let numbers = n_of_m(
        r#""position":"absolute","inset":{"bottom":0,"right":0},"zIndex":-1"#,
        "Inter-Regular",
        "",
        ["Page ", " of "],
        None,
    );
    let overlay = numbers.replacen(r#""type":"text""#, &format!(r#""type":"text"{FIXED}"#), 1);
    for (k, numbers) in painted_numbers(&with_fixed(3, &overlay)).iter().enumerate() {
        assert_eq!(numbers, &[(k + 1).to_string(), "3".to_owned()]);
    }
}

#[test]
fn ids_and_bookmarks_in_overlays_point_at_the_first_page() {
    let overlay = red_box(
        &format!(r#"{FIXED},"id":"stamp","bookmark":{{"title":"Stamp"}}"#),
        r#""position":"absolute","inset":{"bottom":0,"left":0},"width":20,"height":10"#,
    );
    let heading = red_box(r#","bookmark":{"title":"Body"}"#, r#""height":5"#);
    let after = red_box(r#","bookmark":{"title":"After"}"#, r#""height":5"#);
    let json = with_fixed(2, &format!("{heading},{overlay},{after}"));
    render_doc(&json, |pages, marks| {
        assert_eq!(pages.len(), 3);
        // Once each, in document order.
        let titles: Vec<&str> = marks.outline.iter().map(|e| e.title).collect();
        assert_eq!(titles, ["Body", "Stamp", "After"]);
        // PDF coordinates: y up from the bottom of the page.
        let want = Dest {
            page: 0,
            x: M,
            y: M + 10.0,
        };
        assert_eq!(marks.outline[1].dest, want);
        assert_eq!(marks.anchors["stamp"], want);
    });
}
