//! Probe survival: in every context `usvg` copies or moves shapes, the
//! placeholder of a text comes out as a path the PDF writer finds by its
//! geometry, with the paint the text would get. These depend on `usvg`
//! internals (see `rewrite.rs`), so they should fail loudly on an upgrade.

use std::fmt::Write as _;

use usvg::tiny_skia_path::PathSegment;
use usvg::{Group, Node, Paint};

use super::super::svg::{parse_text, parse_with_text};
use super::{Probes, SvgText};
use crate::text::{LaidText, SvgGlyph};

/// A probe found in the tree: the text, the paint style, and the fill
/// and stroke colours `usvg` resolved for it.
#[derive(Debug, Clone, PartialEq)]
struct Found {
    text: u32,
    paint: u32,
    fill: Option<String>,
    stroke: Option<String>,
}

fn color(p: Option<&Paint>) -> Option<String> {
    match p? {
        Paint::Color(c) => Some(format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue)),
        Paint::LinearGradient(_) => Some("linear".into()),
        Paint::RadialGradient(_) => Some("radial".into()),
        Paint::Pattern(_) => Some("pattern".into()),
    }
}

/// The probe `p` is, if it is one.
fn probe(p: &usvg::Path, probes: &Probes) -> Option<(u32, u32)> {
    let segs: Vec<PathSegment> = p.data().segments().collect();
    let [PathSegment::MoveTo(a), PathSegment::LineTo(b), PathSegment::LineTo(c), PathSegment::LineTo(d), PathSegment::Close] =
        segs[..]
    else {
        return None;
    };
    if (a.y, b.x, c.y, d.x) != (b.y, c.x, d.y, a.x) {
        return None;
    }
    probes.get(&[a.x, a.y, c.x, c.y].map(f32::to_bits)).copied()
}

/// Every probe in `g`, its clip paths, masks, patterns and nested SVGs.
fn find(g: &Group, probes: &Probes, out: &mut Vec<Found>) {
    if let Some(c) = g.clip_path() {
        find(c.root(), probes, out);
    }
    if let Some(m) = g.mask() {
        find(m.root(), probes, out);
    }
    for n in g.children() {
        match n {
            Node::Group(child) => find(child, probes, out),
            Node::Path(p) => {
                for paint in [
                    p.fill().map(usvg::Fill::paint),
                    p.stroke().map(usvg::Stroke::paint),
                ]
                .into_iter()
                .flatten()
                {
                    if let Paint::Pattern(pt) = paint {
                        find(pt.root(), probes, out);
                    }
                }
                if let Some((text, paint)) = probe(p, probes) {
                    out.push(Found {
                        text,
                        paint,
                        fill: color(p.fill().map(usvg::Fill::paint)),
                        stroke: color(p.stroke().map(usvg::Stroke::paint)),
                    });
                }
            }
            Node::Image(img) => {
                if let usvg::ImageKind::SVG(tree) = img.kind() {
                    find(tree.root(), probes, out);
                }
            }
            Node::Text(_) => {}
        }
    }
}

/// A layout with the same box for every text, and a glyph per paint
/// style: probes must still be told apart.
fn stub(text: &SvgText) -> LaidText {
    LaidText {
        glyphs: (0..text.paints.len() as u32)
            .map(|paint| SvgGlyph {
                font: 0,
                id: 1,
                size: 10.0,
                x: 0.0,
                y: 0.0,
                rotate: 0.0,
                scale_x: 1.0,
                paint,
                text: (0, 0),
                char: 0,
                upright: false,
            })
            .collect(),
        decorations: Vec::new(),
        images: Vec::new(),
        ink: Some([1.5, -8.25, 30.125, 2.0]),
        text: String::new(),
        actual: Vec::new(),
    }
}

/// The size of the SVG without its text; the probes do not depend on it.
fn size() -> usvg::Size {
    usvg::Size::from_wh(100.0, 100.0).unwrap()
}

/// The probes `body` converts to, in tree order.
fn probes_of(body: &str) -> Vec<Found> {
    let svg =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">{body}</svg>"#);
    let source = parse_text(svg.as_bytes())
        .unwrap()
        .text
        .expect("no text found");
    let (tree, probed) = parse_with_text(&source, size(), &mut stub).unwrap();
    let mut out = Vec::new();
    find(tree.root(), &probed.probes, &mut out);
    out
}

fn found(text: u32, paint: u32, fill: Option<&str>, stroke: Option<&str>) -> Found {
    Found {
        text,
        paint,
        fill: fill.map(str::to_string),
        stroke: stroke.map(str::to_string),
    }
}

const RED: Option<&str> = Some("#ff0000");
const BLUE: Option<&str> = Some("#0000ff");
const BLACK: Option<&str> = Some("#000000");

#[test]
fn a_plain_text() {
    assert_eq!(probes_of("<text>a</text>"), [found(0, 0, BLACK, None)]);
    assert_eq!(
        probes_of(r#"<text fill="red" stroke="blue">a</text>"#),
        [found(0, 0, RED, BLUE)]
    );
}

#[test]
fn copies_by_use_inherit_their_paint() {
    assert_eq!(
        probes_of(
            r##"<defs><text id="t">a</text></defs>
               <use href="#t" fill="red"/><use href="#t" fill="blue"/>"##
        ),
        [found(0, 0, RED, None), found(0, 0, BLUE, None)]
    );
    assert_eq!(
        probes_of(r##"<symbol id="s"><text fill="red">a</text></symbol><use href="#s"/>"##),
        [found(0, 0, RED, None)]
    );
}

/// A `use` that gives the text it copies other fonts draws a copy of
/// its own, with its own texts; one that gives it nothing new draws the
/// text where it is.
#[test]
fn copies_by_use_with_fonts_of_their_own_draw_their_own_texts() {
    assert_eq!(
        probes_of(
            r##"<defs><g id="g"><text>a</text><text fill="blue">b</text></g></defs>
               <use href="#g" font-size="30" fill="red"/><use href="#g" fill="red"/>
               <use href="#g" font-size="30" fill="blue"/>"##
        ),
        [
            found(2, 0, RED, None),
            found(3, 0, BLUE, None),
            found(0, 0, RED, None),
            found(1, 0, BLUE, None),
            found(4, 0, BLUE, None),
            found(5, 0, BLUE, None),
        ]
    );
    assert_eq!(
        probes_of(
            r##"<defs><text id="t">a</text></defs>
               <use href="#t" style="font-family:Mono" fill="red"/>"##
        ),
        [found(1, 0, RED, None)]
    );
    assert_eq!(
        probes_of(
            r##"<symbol id="s" viewBox="0 0 10 10"><text fill="red">a</text></symbol>
               <use href="#s" font-weight="bold" width="20" height="20"/>"##
        ),
        [found(1, 0, RED, None)]
    );
}

#[test]
fn markers_draw_their_text_with_context_paint() {
    let probes = probes_of(
        r##"<marker id="m" markerWidth="10" markerHeight="10">
             <text fill="context-stroke">a</text></marker>
           <path d="M0 0L50 50L90 0" stroke="#0000ff" marker-mid="url(#m)" marker-end="url(#m)"/>"##,
    );
    assert_eq!(probes, [found(0, 0, BLUE, None), found(0, 0, BLUE, None)]);
}

#[test]
fn patterns_masks_and_clip_paths_hold_their_text() {
    assert_eq!(
        probes_of(
            r#"<pattern id="p" width="10" height="10" patternUnits="userSpaceOnUse">
                 <text fill="red">a</text></pattern><rect width="50" height="50" fill="url(#p)"/>"#
        ),
        [found(0, 0, RED, None)]
    );
    assert_eq!(
        probes_of(
            r#"<mask id="m"><text fill="white">a</text></mask>
               <rect width="50" height="50" mask="url(#m)"/>"#
        ),
        [found(0, 0, Some("#ffffff"), None)]
    );
    // In a clip path the placeholder is bare probes, one per paint style;
    // `usvg` fills them black, as it does every shape there.
    assert_eq!(
        probes_of(
            r#"<clipPath id="c"><text>a<tspan fill="red">b</tspan></text></clipPath>
               <rect width="50" height="50" clip-path="url(#c)"/>"#
        ),
        [found(0, 0, BLACK, None), found(0, 1, BLACK, None)]
    );
}

#[test]
fn switch_draws_the_text_it_picks() {
    assert_eq!(
        probes_of(
            r#"<switch><text systemLanguage="xx">a</text><text fill="red">b</text></switch>"#
        ),
        [found(1, 0, RED, None)]
    );
}

#[test]
fn groups_with_opacity_keep_their_text() {
    assert_eq!(
        probes_of(r#"<g opacity="0.5" fill="blue"><text>a</text></g>"#),
        [found(0, 0, BLUE, None)]
    );
}

#[test]
fn author_rules_about_paths_and_links_do_not_reach_probes() {
    assert_eq!(
        probes_of(
            r##"<style>path { fill: none; stroke: red; opacity: 0 } a { fill: green }</style>
               <g fill="blue"><text>a<tspan fill="#ff0000">b</tspan></text></g>"##
        ),
        [found(0, 0, BLUE, None), found(0, 1, RED, None)]
    );
}

/// `usvg` applies the first `!important` declaration it meets, and a
/// sheet comes before `style`: probes are elements those rules do not
/// name.
#[test]
fn important_rules_about_paths_and_links_do_not_reach_probes() {
    assert_eq!(
        probes_of(
            r##"<style>path { fill: none !important; stroke: red !important; opacity: 0 !important }
                 a { fill: green !important; transform: scale(0) !important }</style>
               <g fill="blue"><text>a<tspan fill="#ff0000">b</tspan></text><text>c</text></g>
               <clipPath id="c"><text>d</text></clipPath>
               <rect width="5" height="5" clip-path="url(#c)"/>"##
        ),
        [
            found(0, 0, BLUE, None),
            found(0, 1, RED, None),
            found(1, 0, BLUE, None),
            found(2, 0, BLACK, None),
        ]
    );
}

#[test]
fn nested_svgs_have_their_own_probes() {
    let inner = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text fill="red">a</text></svg>"#;
    let mut url = String::from("data:image/svg+xml,");
    for b in inner.bytes() {
        let _ = write!(url, "%{b:02X}");
    }
    let probes = probes_of(&format!(
        r#"<text fill="blue">b</text><image width="10" height="10" href="{url}"/>"#
    ));
    assert_eq!(probes, [found(0, 0, BLUE, None), found(1, 0, RED, None)]);
}

#[test]
fn reuses_a_conversion_of_the_same_layout() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text>reused</text></svg>"#;
    let source = parse_text(svg.as_bytes()).unwrap().text.unwrap();
    let (a, _) = parse_with_text(&source, size(), &mut stub).unwrap();
    let (b, _) = parse_with_text(&source, size(), &mut stub).unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    // Another layout is another conversion.
    let (c, _) = parse_with_text(&source, size(), &mut |t| LaidText {
        ink: Some([0.0, 0.0, 1.0, 1.0]),
        ..stub(t)
    })
    .unwrap();
    assert!(!std::sync::Arc::ptr_eq(&a, &c));
}

#[test]
fn texts_with_the_same_box_get_their_own_probes() {
    assert_eq!(
        probes_of(r#"<text fill="red">a</text><text fill="blue">a</text>"#),
        [found(0, 0, RED, None), found(1, 0, BLUE, None)]
    );
}

#[test]
fn text_without_ink_keeps_its_id() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text id="t"> </text><use href="#t"/></svg>"##;
    let source = parse_text(svg.as_bytes()).unwrap().text;
    // An empty text is no text at all.
    assert!(source.is_none());
}

/// A text with a `textPath` is a text like any other: its probes carry the
/// paint of its spans, in every context, and the path it follows stays out
/// of the drawing when it is in `<defs>`.
#[test]
fn text_on_a_path_has_its_probes() {
    let path = r#"<defs><path id="p" d="M0 5H50"/></defs>"#;
    assert_eq!(
        probes_of(&format!(
            r##"{path}<text fill="red"><textPath href="#p">a<tspan fill="blue">b</tspan></textPath></text>"##
        )),
        [
            found(0, 0, Some("#ff0000"), None),
            found(0, 1, Some("#0000ff"), None)
        ]
    );
    assert_eq!(
        probes_of(&format!(
            r##"{path}<defs><text id="t"><textPath href="#p">a</textPath></text></defs><use href="#t" fill="red"/>"##
        )),
        [found(0, 0, Some("#ff0000"), None)]
    );
    assert_eq!(
        probes_of(&format!(
            r##"{path}<clipPath id="c"><text><textPath href="#p">a</textPath></text></clipPath><rect width="9" height="9" clip-path="url(#c)"/>"##
        )),
        [found(0, 0, BLACK, None)]
    );
}
