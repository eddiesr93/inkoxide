use std::fmt::Write as _;

use super::*;
use crate::fonts::FontSet;
use crate::images::svg_text::extract;
use crate::text::RustybuzzShaper;
use crate::{FontStore, InputFormat};

const FONTS: &str = r#"[{"id":"Inter-Regular","family":"Inter"},{"id":"Inter-Bold","family":"Inter","weight":700}]"#;

fn store() -> FontStore {
    crate::test_fixtures::fonts(&["Inter-Regular", "Inter-Bold"])
}

fn document(fonts: &str) -> crate::schema::Document {
    let json = format!(
        r#"{{"version":1,"fonts":{fonts},"pages":[{{"size":{{"width":10,"height":10}}}}]}}"#
    );
    crate::validate::parse(json.as_bytes(), InputFormat::Json).unwrap()
}

/// The first text of `body` laid out with Inter, and Inter's scale at
/// `size` with the advance of `c`.
fn laid_with(fonts: &str, body: &str) -> LaidText {
    let src =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">{body}</svg>"#);
    let xml = usvg::roxmltree::Document::parse(&src).unwrap();
    let text = extract(&xml).unwrap().0.texts.remove(0);
    let store = store();
    let doc = document(fonts);
    let set = FontSet::load(&doc, &store).unwrap();
    let mut shaper = RustybuzzShaper::new(&set).unwrap();
    lay_out(&text, &mut shaper)
}

fn laid(body: &str) -> LaidText {
    laid_with(FONTS, body)
}

/// Inter Regular's advance of `c` at `size`, and its metrics in points.
fn advance(c: char, size: f32) -> f32 {
    let store = store();
    let doc = document(FONTS);
    let set = FontSet::load(&doc, &store).unwrap();
    let f = set.get(0);
    f.advance(f.glyph(c).unwrap()) * f.scale(size)
}

fn metric(size: f32, m: impl Fn(&crate::fonts::FontMetrics) -> f32) -> f32 {
    let store = store();
    let data = store.get("Inter-Regular").unwrap();
    m(&data.metrics) * size / data.metrics.units_per_em
}

fn xs(t: &LaidText) -> Vec<f32> {
    t.glyphs.iter().map(|g| g.x).collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn places_glyphs_from_the_text_position() {
    let t = laid(r#"<text x="10" y="50" font-size="20">ab</text>"#);
    assert_eq!(t.glyphs.len(), 2);
    let a = advance('a', 20.0);
    assert_eq!((t.glyphs[0].x, t.glyphs[0].y), (10.0, 50.0));
    assert!(close(t.glyphs[1].x, 10.0 + a));
    assert!(t
        .glyphs
        .iter()
        .all(|g| g.size == 20.0 && g.font == 0 && g.rotate == 0.0));
    // The default size is 16, and the family falls back to the first font.
    let t = laid(r#"<text font-family="Missing">a</text>"#);
    assert_eq!(
        (t.glyphs[0].size, t.glyphs[0].x, t.glyphs[0].y),
        (16.0, 0.0, 0.0)
    );
}

#[test]
fn picks_the_declared_font_by_family_weight_and_style() {
    let t = laid(
        r#"<text font-family="inter" font-weight="bold">a<tspan font-weight="300">b</tspan></text>"#,
    );
    let fonts: Vec<FontKey> = t.glyphs.iter().map(|g| g.font).collect();
    assert_eq!(fonts, [1, 0]);
}

#[test]
fn anchors_each_chunk() {
    let w = advance('a', 10.0) + advance('b', 10.0);
    let t = laid(r#"<text x="100" font-size="10" text-anchor="middle">ab</text>"#);
    assert!(close(t.glyphs[0].x, 100.0 - w / 2.0), "{:?}", xs(&t));
    let t = laid(r#"<text x="100" font-size="10" text-anchor="end">ab</text>"#);
    assert!(close(t.glyphs[0].x, 100.0 - w));
    // A new chunk at each absolute position, anchored on its own.
    let t = laid(r#"<text x="10 100" font-size="10" text-anchor="end">aab</text>"#);
    let a = advance('a', 10.0);
    assert!(close(t.glyphs[0].x, 10.0 - a), "{:?}", xs(&t));
    assert!(close(t.glyphs[1].x, 100.0 - w), "{:?}", xs(&t));
    // Right to left, the start is on the right.
    let t = laid(r#"<text x="100" font-size="10" direction="rtl">ab</text>"#);
    assert!(close(
        t.glyphs.iter().map(|g| g.x).fold(f32::MAX, f32::min),
        100.0 - w
    ));
}

#[test]
fn shifts_and_rotates_characters() {
    let t =
        laid(r#"<text x="10" y="20" font-size="10" dx="0 5" dy="0 2" rotate="0 30 0">abc</text>"#);
    let (a, b) = (advance('a', 10.0), advance('b', 10.0));
    assert!(close(t.glyphs[1].x, 10.0 + a + 5.0));
    // Relative shifts carry on to the characters after them.
    assert!(close(t.glyphs[2].x, 10.0 + a + 5.0 + b));
    assert_eq!([t.glyphs[1].y, t.glyphs[2].y], [22.0, 22.0]);
    let r: Vec<f32> = t.glyphs.iter().map(|g| g.rotate).collect();
    assert_eq!(r, [0.0, 30.0, 0.0]);
    // A chunk without a position goes on where the last one ended.
    let t = laid(r#"<text x="10" y="20" font-size="10">a<tspan y="40">b</tspan></text>"#);
    assert!(close(t.glyphs[1].x, 10.0 + a));
    assert_eq!(t.glyphs[1].y, 40.0);
}

#[test]
fn spaces_letters_and_words() {
    let (a, space) = (advance('a', 10.0), advance(' ', 10.0));
    let t = laid(r#"<text font-size="10" letter-spacing="2" word-spacing="5">a a</text>"#);
    assert!(close(t.glyphs[1].x, a + 2.0));
    assert!(close(t.glyphs[2].x, a + 2.0 + space + 2.0 + 5.0));
    // No spacing after the last letter: an end anchor lines up its edge.
    let t = laid(r#"<text x="100" font-size="10" letter-spacing="3" text-anchor="end">aa</text>"#);
    assert!(close(t.glyphs[1].x + a, 100.0), "{:?}", xs(&t));
}

#[test]
fn fits_text_length() {
    let a = advance('a', 10.0);
    let t = laid(r#"<text font-size="10" textLength="100">aaa</text>"#);
    assert!(close(t.glyphs[2].x + a, 100.0), "{:?}", xs(&t));
    assert!(t.glyphs.iter().all(|g| g.scale_x == 1.0));
    let t =
        laid(r#"<text font-size="10" textLength="100" lengthAdjust="spacingAndGlyphs">aaa</text>"#);
    let factor = 100.0 / (3.0 * a);
    assert!(t.glyphs.iter().all(|g| close(g.scale_x, factor)));
    assert!(close(t.glyphs[2].x, 2.0 * a * factor));
}

#[test]
fn moves_baselines_with_the_font_metrics() {
    let ascent = metric(10.0, |m| m.ascender);
    let t = laid(r#"<text y="50" font-size="10" dominant-baseline="text-before-edge">a</text>"#);
    assert!(close(t.glyphs[0].y, 50.0 + ascent));
    let xh = metric(10.0, |m| m.x_height);
    let t = laid(r#"<text y="50" font-size="10" dominant-baseline="middle">a</text>"#);
    assert!(close(t.glyphs[0].y, 50.0 + xh / 2.0));
    let sup = metric(10.0, |m| m.superscript_offset);
    let sub = metric(10.0, |m| m.subscript_offset);
    let t = laid(
        r#"<text y="50" font-size="10">a<tspan baseline-shift="super">b</tspan><tspan baseline-shift="sub">c</tspan><tspan baseline-shift="3">d</tspan></text>"#,
    );
    let ys: Vec<f32> = t.glyphs.iter().map(|g| g.y).collect();
    assert!(close(ys[1], 50.0 - sup), "{ys:?}");
    assert!(close(ys[2], 50.0 + sub), "{ys:?}");
    assert!(close(ys[3], 47.0), "{ys:?}");
}

#[test]
fn draws_decorations_along_the_baseline() {
    let w = 2.0 * advance('a', 10.0);
    let pos = metric(10.0, |m| m.underline_position);
    let thick = metric(10.0, |m| m.underline_thickness);
    let t = laid(
        r#"<text x="5" y="50" font-size="10" text-decoration="underline line-through">aa</text>"#,
    );
    assert_eq!(t.decorations.len(), 2);
    let u = t.decorations[0];
    assert!(!u.over && t.decorations[1].over);
    let [[x0, y0], [x1, _], [_, y1], _] = u.quad;
    assert!(close(x0, 5.0) && close(x1, 5.0 + w));
    assert!(close(y0, 50.0 - pos - thick / 2.0) && close(y1, 50.0 - pos + thick / 2.0));
}

#[test]
fn measures_the_ink() {
    let t = laid(r#"<text x="10" y="50" font-size="100">x</text>"#);
    let [x0, y0, x1, y1] = t.ink.unwrap();
    let xh = metric(100.0, |m| m.x_height);
    assert!(x0 >= 10.0 && x1 <= 10.0 + advance('x', 100.0) + 1.0);
    assert!(
        (y0 - (50.0 - xh)).abs() < 1.0 && (y1 - 50.0).abs() < 1.0,
        "{:?}",
        t.ink
    );
    // Spaces have no ink.
    assert_eq!(laid(r#"<text xml:space="preserve">  </text>"#).ink, None);
}

#[test]
fn draws_nothing_without_fonts_or_a_size() {
    assert_eq!(laid_with("[]", "<text>a</text>"), LaidText::default());
    assert!(laid(r#"<text font-size="0">a</text>"#).glyphs.is_empty());
    assert!(laid(r#"<text font-size="-2">a</text>"#).glyphs.is_empty());
    let t = laid(r#"<text>a<tspan display="none">b</tspan>c</text>"#);
    assert_eq!(t.glyphs.len(), 2);
}

#[test]
fn records_glyphs_for_the_text_layer() {
    // Shaping through the render's shaper maps each glyph to its text.
    let src = r#"<svg xmlns="http://www.w3.org/2000/svg"><text>Hi</text></svg>"#;
    let xml = usvg::roxmltree::Document::parse(src).unwrap();
    let text = extract(&xml).unwrap().0.texts.remove(0);
    let store = store();
    let doc = document(FONTS);
    let set = FontSet::load(&doc, &store).unwrap();
    let mut cache = crate::text::TextCache::new(RustybuzzShaper::new(&set).unwrap(), None);
    let laid = lay_out(&text, cache.shaper());
    let unicode = cache.into_unicode();
    let map = &unicode.fonts[&0];
    let text: String = laid.glyphs.iter().map(|g| map[&g.id].as_str()).collect();
    assert_eq!(text, "Hi");
}

/// Each decoration finds its font without a search: thousands of
/// decorated runs (a `dx` starts each) lay out in linear time, and each
/// paint style's glyphs are found in one step.
#[test]
fn decorates_and_sorts_many_spans_in_linear_time() {
    let mut body = String::from(r#"<text text-decoration="underline">"#);
    for i in 0..20_000 {
        let fill = if i % 2 == 0 { "red" } else { "blue" };
        let _ = write!(body, r#"<tspan dx="1" fill="{fill}">a</tspan>"#);
    }
    body.push_str("</text>");
    let t = std::time::Instant::now();
    let laid = laid(&body);
    assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
    assert_eq!(laid.decorations.len(), 20_000);
    assert_eq!(laid.glyphs_of(0).len(), 10_000);
    assert!(laid.glyphs_of(1).iter().all(|g| g.paint == 1));
    assert_eq!(laid.decorations_of(1).len(), 10_000);
    // In each paint style, in the order they are drawn.
    assert!(laid.glyphs_of(0).windows(2).all(|w| w[0].x < w[1].x));
}

/// An emoji the document maps to an image is that image, 1em high and
/// as wide as its aspect ratio, in the place of a glyph, with its text.
#[test]
fn lays_out_emoji_images_as_images() {
    let src = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text x="10" y="50" font-size="20" fill="red">a😀b</text></svg>"#;
    let xml = usvg::roxmltree::Document::parse(src).unwrap();
    let text = extract(&xml).unwrap().0.texts.remove(0);
    let store = store();
    let doc = document(FONTS);
    let set = FontSet::load(&doc, &store).unwrap();
    let source = crate::text::ObjectSource {
        key: 7,
        intrinsic: (40.0, 20.0),
    };
    let emoji = std::collections::HashMap::from([("😀".to_string(), source)]);
    let objects = crate::text::InlineObjects::new(std::collections::HashMap::new(), emoji);
    let mut shaper = RustybuzzShaper::new(&set)
        .unwrap()
        .with_objects(std::sync::Arc::new(objects));
    let laid = lay_out(&text, &mut shaper);
    assert_eq!(laid.images.len(), 1);
    let image = &laid.images[0];
    assert_eq!(image.key, 7);
    assert_eq!(image.text.as_deref(), Some("😀"));
    assert!(close(image.x, 10.0 + advance('a', 20.0)));
    // 1em high, its bottom 0.1em below the baseline.
    assert_eq!(image.rect, crate::paint::Rect::new(0.0, -18.0, 40.0, 20.0));
    // Two glyphs, none missing, the second after the image.
    assert_eq!(laid.glyphs.len(), 2);
    assert!(laid.glyphs.iter().all(|g| g.id != 0));
    assert!(close(laid.glyphs[1].x, image.x + 40.0));
    let ink = laid.ink.unwrap();
    assert!(ink[1] <= 50.0 - 18.0 && ink[2] >= image.x + 40.0, "{ink:?}");
}
