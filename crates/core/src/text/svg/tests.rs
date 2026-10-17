use std::fmt::Write as _;

use super::*;
use crate::fonts::FontSet;
use crate::images::svg_text::extract;
use crate::text::RustybuzzShaper;
use crate::{FontStore, InputFormat};

const FONTS: &str = r#"[{"id":"Inter-Regular","family":"Inter"},{"id":"Inter-Bold","family":"Inter","weight":700}]"#;

fn store() -> FontStore {
    crate::test_fixtures::fonts(&["Inter-Regular", "Inter-Bold", "VertTest"])
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
    let text = extract(&xml, usize::MAX).unwrap().0.texts.remove(0);
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
    let text = extract(&xml, usize::MAX).unwrap().0.texts.remove(0);
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

/// Each `textLength` finds the clusters inside it without going through
/// the others: many spans with their own lay out in linear time.
#[test]
fn fits_many_text_lengths_in_linear_time() {
    let a = advance('a', 10.0);
    crate::test_timing::assert_linear("text lengths", 8_000, |n| {
        let mut body = String::from(r#"<text font-size="10">"#);
        for _ in 0..n {
            body.push_str(r#"<tspan textLength="5" lengthAdjust="spacingAndGlyphs">a</tspan>"#);
        }
        body.push_str("</text>");
        let t = std::time::Instant::now();
        let laid = laid(&body);
        let took = t.elapsed();
        assert_eq!(laid.glyphs.len(), n);
        assert!(laid.glyphs.iter().all(|g| close(g.scale_x, 5.0 / a)));
        assert!(close(laid.glyphs[n - 1].x, 5.0 * (n - 1) as f32));
        took
    });
}

/// Each decoration finds its font without a search: decorated runs (a
/// `dx` starts each) lay out in linear time, and each paint style's glyphs
/// are found in one step.
#[test]
fn decorates_and_sorts_many_spans_in_linear_time() {
    crate::test_timing::assert_linear("decorated spans", 5_000, |n| {
        let mut body = String::from(r#"<text text-decoration="underline">"#);
        for i in 0..n {
            let fill = if i % 2 == 0 { "red" } else { "blue" };
            let _ = write!(body, r#"<tspan dx="1" fill="{fill}">a</tspan>"#);
        }
        body.push_str("</text>");
        let t = std::time::Instant::now();
        let laid = laid(&body);
        let took = t.elapsed();
        assert_eq!(laid.decorations.len(), n);
        assert_eq!(laid.glyphs_of(0).len(), n / 2);
        assert!(laid.glyphs_of(1).iter().all(|g| g.paint == 1));
        assert_eq!(laid.decorations_of(1).len(), n / 2);
        // In each paint style, in the order they are drawn.
        assert!(laid.glyphs_of(0).windows(2).all(|w| w[0].x < w[1].x));
        took
    });
}

/// An emoji the document maps to an image is that image, 1em high and
/// as wide as its aspect ratio, in the place of a glyph, with its text.
#[test]
fn lays_out_emoji_images_as_images() {
    let src = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text x="10" y="50" font-size="20" fill="red">a😀b</text></svg>"#;
    let xml = usvg::roxmltree::Document::parse(src).unwrap();
    let text = extract(&xml, usize::MAX).unwrap().0.texts.remove(0);
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
    assert_eq!(image.rect, crate::geom::Rect::new(0.0, -18.0, 40.0, 20.0));
    // Two glyphs, none missing, the second after the image.
    assert_eq!(laid.glyphs.len(), 2);
    assert!(laid.glyphs.iter().all(|g| g.id != 0));
    assert!(close(laid.glyphs[1].x, image.x + 40.0));
    let ink = laid.ink.unwrap();
    assert!(ink[1] <= 50.0 - 18.0 && ink[2] >= image.x + 40.0, "{ink:?}");
}

/// Fonts for vertical text: the test font with vertical metrics and forms
/// first, then Inter.
const VERT: &str = r#"[{"id":"VertTest","family":"Vert"},{"id":"Inter-Regular","family":"Inter"}]"#;

/// The first text of `body`, vertical (`vertical-rl`) at (50, 10) in the
/// vertical test font at size 10, laid out.
fn column(attrs: &str, body: &str) -> LaidText {
    laid_with(
        VERT,
        &format!(
            r#"<text x="50" y="10" font-size="10" font-family="Vert" writing-mode="vertical-rl" {attrs}>{body}</text>"#
        ),
    )
}

/// The vertical test font's central baseline below its alphabetic one at
/// size 10: the middle of its ascender and descender.
fn central() -> f32 {
    let store = store();
    let m = &store.get("VertTest").unwrap().metrics;
    f32::midpoint(m.ascender, m.descender) * 10.0 / m.units_per_em
}

#[test]
fn stands_upright_characters_down_the_column() {
    let t = column("", "縦書");
    // The column's centre is x; each ideograph advances 10 down, its
    // origin half its width left of the centre and 8.8 below its top.
    let at: Vec<(f32, f32, f32)> = t.glyphs.iter().map(|g| (g.x, g.y, g.rotate)).collect();
    assert!(close(at[0].0, 45.0) && close(at[0].1, 18.8), "{at:?}");
    assert!(close(at[1].0, 45.0) && close(at[1].1, 28.8), "{at:?}");
    assert!(at.iter().all(|g| g.2 == 0.0));
    // The same in `tb` and `vertical-lr`, which one line cannot tell apart.
    for mode in ["tb", "tb-rl", "vertical-lr"] {
        let other = laid_with(
            VERT,
            &format!(
                r#"<text x="50" y="10" font-size="10" font-family="Vert" writing-mode="{mode}">縦書</text>"#
            ),
        );
        assert_eq!(other.glyphs, t.glyphs, "{mode}");
    }
}

#[test]
fn turns_punctuation_into_its_vertical_forms() {
    let ids = |t: &LaidText| t.glyphs.iter().map(|g| g.id).collect::<Vec<_>>();
    let horizontal = laid_with(
        VERT,
        r#"<text font-size="10" font-family="Vert">、「ー」。</text>"#,
    );
    let vertical = column("", "、「ー」。");
    assert!(ids(&vertical)
        .iter()
        .zip(ids(&horizontal))
        .all(|(v, h)| *v != h));
    assert!(vertical.glyphs.iter().all(|g| g.rotate == 0.0));
    // The text layer keeps the characters.
    assert_eq!(vertical.text, "、「ー」。");
}

#[test]
fn lays_latin_sideways_in_mixed_orientation() {
    let t = column("", "縦AB〈書");
    let rot: Vec<f32> = t.glyphs.iter().map(|g| g.rotate).collect();
    // 〈 has no vertical form in the font: it turns, as Latin does.
    assert_eq!(rot, [0.0, 90.0, 90.0, 90.0, 0.0]);
    // A sideways glyph lies on the central baseline, down the column.
    let a = &t.glyphs[1];
    assert!(close(a.x, 50.0 - central()) && close(a.y, 20.0), "{a:?}");
    // Sideways runs advance by their horizontal advances (A, B 6; 〈 10).
    let last = &t.glyphs[4];
    assert!(
        close(last.y, 10.0 + 10.0 + 6.0 + 6.0 + 10.0 + 8.8),
        "{last:?}"
    );
    // `upright` stands them all; `sideways` lays them all down.
    let up = column(r#"text-orientation="upright""#, "AB");
    assert!(up.glyphs.iter().all(|g| g.rotate == 0.0));
    assert!(close(up.glyphs[1].y - up.glyphs[0].y, 10.0));
    let down = column(r#"text-orientation="sideways""#, "縦書");
    assert!(down.glyphs.iter().all(|g| g.rotate == 90.0));
    // The SVG 1.1 property says the same.
    let up_too = column(r#"glyph-orientation-vertical="0""#, "AB");
    assert_eq!(up_too.glyphs, up.glyphs);
}

#[test]
fn sideways_modes_turn_every_character() {
    let rl = laid_with(
        VERT,
        r#"<text x="50" y="10" font-size="10" font-family="Vert" writing-mode="sideways-rl">縦A</text>"#,
    );
    assert!(rl.glyphs.iter().all(|g| g.rotate == 90.0));
    assert!(rl.glyphs[1].y > rl.glyphs[0].y);
    // `sideways-lr` goes up, turned the other way.
    let lr = laid_with(
        VERT,
        r#"<text x="50" y="100" font-size="10" font-family="Vert" writing-mode="sideways-lr">縦A</text>"#,
    );
    assert!(lr.glyphs.iter().all(|g| g.rotate == -90.0));
    assert!(
        close(lr.glyphs[0].y, 100.0) && close(lr.glyphs[1].y, 90.0),
        "{:?}",
        lr.glyphs
    );
    assert!(close(lr.glyphs[0].x, 50.0 + central()));
}

#[test]
fn anchors_shifts_and_lengths_act_along_the_column() {
    let t = column(r#"text-anchor="middle""#, "縦書");
    assert!(close(t.glyphs[0].y, 10.0 - 10.0 + 8.8), "{:?}", t.glyphs);
    let t = column(r#"text-anchor="end""#, "縦書");
    assert!(close(t.glyphs[1].y, 10.0 - 10.0 + 8.8), "{:?}", t.glyphs);
    // dx moves across, dy along; both carry on.
    let t = column(r#"dx="0 3" dy="0 2""#, "縦書文");
    assert!(
        close(t.glyphs[1].x, 48.0) && close(t.glyphs[1].y, 30.8),
        "{:?}",
        t.glyphs
    );
    assert!(
        close(t.glyphs[2].x, 48.0) && close(t.glyphs[2].y, 40.8),
        "{:?}",
        t.glyphs
    );
    // `textLength` spreads the characters down the column.
    let t = column(r#"textLength="40""#, "縦書文");
    assert!(
        close(t.glyphs[2].y - t.glyphs[0].y, 40.0 - 10.0),
        "{:?}",
        t.glyphs
    );
    // A new chunk starts at each absolute position.
    let t = column("", r#"縦<tspan y="100">書</tspan>"#);
    assert!(close(t.glyphs[1].y, 108.8));
}

#[test]
fn decorates_vertical_text_along_the_column() {
    let t = column(r#"text-decoration="underline overline""#, "縦書");
    assert_eq!(t.decorations.len(), 2);
    let xs = |d: &SvgDecoration| d.quad.map(|p| p[0]);
    let ys = |d: &SvgDecoration| d.quad.map(|p| p[1]);
    // The underline on the left of the column, the overline on the right,
    // both as long as the text.
    let (under, over) = (&t.decorations[0], &t.decorations[1]);
    assert!(xs(under).iter().all(|&x| x < 50.0), "{under:?}");
    assert!(xs(over).iter().all(|&x| x > 50.0), "{over:?}");
    let span = |d: &SvgDecoration| {
        let y = ys(d);
        y.iter().copied().fold(f32::MIN, f32::max) - y.iter().copied().fold(f32::MAX, f32::min)
    };
    assert!(close(span(under), 20.0));
    // The ink covers the column.
    let ink = t.ink.unwrap();
    assert!(
        ink[0] < 46.0 && ink[2] > 54.0 && ink[1] <= 10.0 && ink[3] >= 30.0,
        "{ink:?}"
    );
}

#[test]
fn a_straight_path_lays_out_as_a_line() {
    let on_path = laid(
        r##"<defs><path id="p" d="M10 50H200"/></defs><text font-size="10"><textPath href="#p">ab c</textPath></text>"##,
    );
    let line = laid(r#"<text x="10" y="50" font-size="10">ab c</text>"#);
    assert_eq!(on_path.glyphs.len(), line.glyphs.len());
    for (p, l) in on_path.glyphs.iter().zip(&line.glyphs) {
        assert!(
            close(p.x, l.x) && close(p.y, l.y) && close(p.rotate, 0.0),
            "{p:?} {l:?}"
        );
    }
    assert_eq!(on_path.text, line.text);
}

#[test]
fn turns_glyphs_to_the_path() {
    // A circle of radius 50 about (100, 100), from its rightmost point,
    // clockwise: down first.
    let t = laid(
        r##"<circle id="c" cx="100" cy="100" r="50"/><text font-size="10"><textPath href="#c">aaaa</textPath></text>"##,
    );
    let a = advance('a', 10.0);
    for (i, g) in t.glyphs.iter().enumerate() {
        // The middle of each glyph's advance is on the circle, and the
        // glyph turns with the tangent there.
        let mid = (i as f32 + 0.5) * a;
        let theta = mid / 50.0;
        assert!(
            (g.rotate - (90.0 + theta.to_degrees())).abs() < 0.1,
            "{i}: {g:?}"
        );
        let (sin, cos) = (g.rotate.to_radians().sin(), g.rotate.to_radians().cos());
        let (mx, my) = (g.x + a / 2.0 * cos, g.y + a / 2.0 * sin);
        let (px, py) = (100.0 + 50.0 * theta.cos(), 100.0 + 50.0 * theta.sin());
        // Within what cubic curves are off a circle (0.03%).
        assert!(
            (mx - px).abs() < 0.02 && (my - py).abs() < 0.02,
            "{i}: {g:?}"
        );
    }
}

#[test]
fn drops_what_is_off_the_path_and_goes_on_after_it() {
    let a = advance('a', 10.0);
    // A path one and a half glyphs long: the second glyph's middle is
    // past its end.
    let t = laid(&format!(
        r##"<path id="p" d="M0 20H{}"/><text font-size="10"><textPath href="#p">aa</textPath>b</text>"##,
        a * 1.4
    ));
    assert_eq!(t.glyphs.len(), 2, "{:?}", t.glyphs);
    assert_eq!(t.text, "ab");
    // Text after the textPath goes on past the last glyph drawn.
    assert!(close(t.glyphs[1].x, a / 2.0 + a), "{:?}", t.glyphs);
    // A start before the path hides what is before it.
    let t = laid(
        r##"<path id="p" d="M0 20H100"/><text font-size="10"><textPath href="#p" startOffset="-100">aa</textPath></text>"##,
    );
    assert!(t.glyphs.is_empty());
}

#[test]
fn anchors_and_shifts_along_the_path() {
    let w = 2.0 * advance('a', 10.0);
    let t = laid(
        r##"<path id="p" d="M0 20H100"/><text font-size="10" text-anchor="middle"><textPath href="#p" startOffset="50%">aa</textPath></text>"##,
    );
    assert!(close(t.glyphs[0].x, 50.0 - w / 2.0), "{:?}", t.glyphs);
    // x on a span adds to the start; dx moves along, dy off the path.
    let t = laid(
        r##"<path id="p" d="M0 20V100"/><text font-size="10"><textPath href="#p"><tspan x="10" dx="5" dy="3">a</tspan></textPath></text>"##,
    );
    let g = &t.glyphs[0];
    assert!(
        close(g.rotate, 90.0) && close(g.y, 35.0) && close(g.x, -3.0),
        "{g:?}"
    );
    // Each glyph has its own decoration.
    let t = laid(
        r##"<path id="p" d="M0 20H100"/><text font-size="10" text-decoration="underline"><textPath href="#p">aaa</textPath></text>"##,
    );
    assert_eq!(t.decorations.len(), 3);
}

#[test]
fn shapes_with_font_features() {
    // Inter's digits are proportional; `tnum` makes them as wide as each
    // other.
    let widths = |attrs: &str| -> Vec<f32> {
        let t = laid(&format!(r#"<text font-size="10" {attrs}>1100</text>"#));
        t.glyphs.windows(2).map(|w| w[1].x - w[0].x).collect()
    };
    let proportional = widths("");
    let tabular = widths(r#"font-variant="tabular-nums""#);
    assert!(!close(proportional[0], proportional[2]), "{proportional:?}");
    assert!(close(tabular[0], tabular[2]), "{tabular:?}");
    assert_eq!(widths(r#"font-feature-settings="'tnum'""#), tabular);
    // Without kerning, "AV" is wider.
    let width = |attrs: &str| {
        let t = laid(&format!(r#"<text font-size="10" {attrs}>AV</text>"#));
        t.glyphs[1].x
    };
    assert!(width(r#"font-kerning="none""#) > width(""));
}
