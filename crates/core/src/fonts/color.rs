//! Colour glyphs: `COLR` layers (versions 0 and 1) written as SVG, and
//! PNG bitmaps from `CBDT` or `sbix`. The text painter draws them as
//! images in place of the glyphs.
//!
//! The SVG keeps what the PDF can show: outlines filled with solid
//! colours and linear and radial gradients, clips, and transforms. A
//! sweep gradient fills with its first stop, and every composite mode
//! draws as source-over. Fonts are untrusted, and `COLR` v1 glyphs nest
//! (a layer can paint another colour glyph, whose layers paint others),
//! so the work per glyph is bounded: past [`MAX_PAINT_OPS`] painter calls
//! or [`MAX_SVG_BYTES`] of markup, the glyph has no colour.
//!
//! Layers in the foreground colour (palette index 0xFFFF) take the text
//! colour, as in browsers: such a glyph is painted once per text colour
//! it is drawn in.

use std::fmt::Write;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

use ttf_parser::colr::{ClipBox, CompositeMode, GradientExtend, Paint, Painter};
use ttf_parser::{GlyphId, RasterImageFormat, RgbaColor, Transform};

use super::LoadedFont;
use crate::paint::{invert, mul};

/// Most painter calls (outlines, fills, clips, layers, transforms) for one
/// colour glyph. Emoji take a few hundred.
pub(crate) const MAX_PAINT_OPS: usize = 10_000;
/// Most SVG markup for one colour glyph.
pub(crate) const MAX_SVG_BYTES: usize = 1 << 20;

/// A colour glyph: an image and the box it fills, `[x_min, y_min, x_max,
/// y_max]` in font units, y up from the baseline.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ColorGlyph {
    Svg { svg: String, bbox: [f32; 4] },
    Png { png: Vec<u8>, bbox: [f32; 4] },
}

/// A text colour, `[r, g, b]`: what the layers of a `COLR` glyph in the
/// foreground colour are painted in. Its alpha is the text's opacity, which
/// the painter applies to the whole glyph.
pub(crate) type TextColor = [u8; 3];

/// The text colour of colour glyphs drawn without one (black).
pub(crate) const BLACK_TEXT: TextColor = [0, 0, 0];

impl LoadedFont<'_> {
    /// Glyph `gid` in colour: its `COLR` layers (those in the foreground
    /// colour black), else its largest PNG bitmap. `None` for a glyph
    /// without colour, or whose colour data is malformed or over the
    /// limits.
    pub fn color_glyph(&self, gid: u16) -> Option<ColorGlyph> {
        self.color_glyph_in(gid, BLACK_TEXT)
    }

    /// Glyph `gid` in colour, as [`Self::color_glyph`], with its layers in
    /// the foreground colour in text colour `text`.
    pub fn color_glyph_in(&self, gid: u16, text: TextColor) -> Option<ColorGlyph> {
        self.painted(gid, text).map(|(glyph, _)| glyph)
    }

    /// Glyph `gid` in colour, as [`Self::color_glyph`], and whether it
    /// takes the text colour: whether it has layers in the foreground
    /// colour.
    pub fn color_glyph_tinted(&self, gid: u16) -> Option<(ColorGlyph, bool)> {
        let (black, maybe) = self.painted(gid, BLACK_TEXT)?;
        // Painted black somewhere: the foreground, or the palette's own
        // black, which white tells apart.
        let tinted = maybe && self.color_glyph_in(gid, [255, 255, 255]).as_ref() != Some(&black);
        Some((black, tinted))
    }

    /// Glyph `gid` in colour with text colour `text`, and whether it paints
    /// in that colour anywhere (a `COLR` layer in the foreground colour, or
    /// one of the palette's that happens to match).
    fn painted(&self, gid: u16, text: TextColor) -> Option<(ColorGlyph, bool)> {
        if !self.has_color() {
            return None;
        }
        let id = GlyphId(gid);
        if self.face.is_color_glyph(id) {
            return colr_svg(&self.face, id, text);
        }
        let img = self.face.glyph_raster_image(id, u16::MAX)?;
        if img.format != RasterImageFormat::PNG
            || img.width == 0
            || img.height == 0
            || img.pixels_per_em == 0
        {
            return None;
        }
        let s = self.data.metrics.units_per_em / f32::from(img.pixels_per_em);
        let (x, y) = (f32::from(img.x) * s, f32::from(img.y) * s);
        let png = ColorGlyph::Png {
            png: img.data.to_vec(),
            bbox: [
                x,
                y,
                x + f32::from(img.width) * s,
                y + f32::from(img.height) * s,
            ],
        };
        Some((png, false))
    }
}

/// Why painting stopped: unwound out of `ttf-parser`, which has no way to
/// stop a painter, and caught in [`colr_svg`].
struct OverBudget;

/// Glyph `id` of `face` as SVG, its layers in the foreground colour in
/// `text`, and whether anything was painted in `text`.
fn colr_svg(
    face: &ttf_parser::Face<'_>,
    id: GlyphId,
    text: TextColor,
) -> Option<(ColorGlyph, bool)> {
    let mut painter = SvgPainter::new(face, text);
    let [r, g, b] = text;
    let foreground = RgbaColor::new(r, g, b, 255);
    let painted = catch_unwind(AssertUnwindSafe(|| {
        face.paint_color_glyph(id, 0, foreground, &mut painter)
    }));
    match painted {
        Ok(Some(())) => {
            let seen = painter.text_seen;
            painter.finish().map(|glyph| (glyph, seen))
        }
        Ok(None) => None,
        Err(payload) if payload.is::<OverBudget>() => None,
        Err(payload) => resume_unwind(payload),
    }
}

/// The 2×3 matrix of `t`, `[a b c d e f]`.
fn matrix(t: Transform) -> [f32; 6] {
    [t.a, t.b, t.c, t.d, t.e, t.f]
}

fn apply(m: [f32; 6], (x, y): (f32, f32)) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Writes glyph outlines as SVG path data, and their bounds.
struct PathData<'s> {
    d: &'s mut String,
}

impl ttf_parser::OutlineBuilder for PathData<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.d, "M{x} {y}");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.d, "L{x} {y}");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(self.d, "Q{x1} {y1} {x} {y}");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(self.d, "C{x1} {y1} {x2} {y2} {x} {y}");
    }
    fn close(&mut self) {
        self.d.push('Z');
    }
}

/// A `COLR` painter writing SVG in font units, y up.
struct SvgPainter<'a, 'f> {
    face: &'f ttf_parser::Face<'a>,
    body: String,
    /// Path data of the last outline, its bounds, and the transform it was
    /// outlined under.
    path: String,
    path_bounds: Option<[f32; 4]>,
    path_transform: [f32; 6],
    transform: [f32; 6],
    transforms: Vec<[f32; 6]>,
    /// The glyph's clip box, when the font gives one.
    clip_box: Option<[f32; 4]>,
    /// The union of what was filled, under its transforms.
    bounds: Option<[f32; 4]>,
    ids: usize,
    ops: usize,
    /// The text colour, and whether a fill or a stop was in it.
    text: TextColor,
    text_seen: bool,
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

impl<'a, 'f> SvgPainter<'a, 'f> {
    fn new(face: &'f ttf_parser::Face<'a>, text: TextColor) -> Self {
        Self {
            face,
            body: String::new(),
            path: String::new(),
            path_bounds: None,
            path_transform: IDENTITY,
            transform: IDENTITY,
            transforms: Vec::new(),
            clip_box: None,
            bounds: None,
            ids: 0,
            ops: 0,
            text,
            text_seen: false,
        }
    }

    /// `c` as an SVG colour, noting whether it is the text colour.
    fn rgb(&mut self, c: RgbaColor) -> String {
        self.text_seen |= [c.red, c.green, c.blue] == self.text;
        format!("rgb({},{},{})", c.red, c.green, c.blue)
    }

    /// Counts one call against the budget, and unwinds past it.
    fn spend(&mut self) {
        self.ops += 1;
        if self.ops > MAX_PAINT_OPS || self.body.len() > MAX_SVG_BYTES {
            resume_unwind(Box::new(OverBudget));
        }
    }

    fn next_id(&mut self, prefix: char) -> String {
        self.ids += 1;
        format!("{prefix}{}", self.ids)
    }

    fn transform_attr(&mut self, name: &str, m: [f32; 6]) {
        if m != IDENTITY {
            let [a, b, c, d, e, f] = m;
            let _ = write!(self.body, r#" {name}="matrix({a} {b} {c} {d} {e} {f})""#);
        }
    }

    /// Fills the last outline with `fill` (a colour or `url(#id)`).
    fn fill_path(&mut self, fill: &str, opacity: f32) {
        if self.path.is_empty() {
            return;
        }
        if let Some([x0, y0, x1, y1]) = self.path_bounds {
            let m = self.path_transform;
            for corner in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                let (x, y) = apply(m, corner);
                self.bounds = Some(match self.bounds {
                    Some([a, b, c, d]) => [a.min(x), b.min(y), c.max(x), d.max(y)],
                    None => [x, y, x, y],
                });
            }
        }
        let _ = write!(self.body, r#"<path d="{}" fill="{fill}""#, self.path);
        if opacity < 1.0 {
            let _ = write!(self.body, r#" fill-opacity="{opacity}""#);
        }
        let m = self.path_transform;
        self.transform_attr("transform", m);
        self.body.push_str("/>");
    }

    fn stops(&mut self, stops: impl Iterator<Item = ttf_parser::colr::ColorStop>) {
        let mut stops: Vec<_> = stops.collect();
        stops.sort_by(|a, b| a.stop_offset.total_cmp(&b.stop_offset));
        for s in stops {
            let c = s.color;
            let color = self.rgb(c);
            let _ = write!(
                self.body,
                r#"<stop offset="{}" stop-color="{color}" stop-opacity="{}"/>"#,
                s.stop_offset,
                f32::from(c.alpha) / 255.0
            );
        }
    }

    /// The gradient's coordinates are in paint space (the transform in
    /// effect); the path is drawn under the transform it was outlined in.
    fn gradient_transform(&mut self) -> bool {
        let Some(inverse) = invert(self.path_transform) else {
            return false;
        };
        let m = mul(inverse, self.transform);
        self.transform_attr("gradientTransform", m);
        true
    }

    fn spread(extend: GradientExtend) -> &'static str {
        match extend {
            GradientExtend::Pad => "pad",
            GradientExtend::Repeat => "repeat",
            GradientExtend::Reflect => "reflect",
        }
    }

    fn clip(&mut self, path: &str, transform: [f32; 6]) {
        let id = self.next_id('c');
        let _ = write!(self.body, r#"<clipPath id="{id}"><path d="{path}""#);
        self.transform_attr("transform", transform);
        let _ = write!(self.body, r#"/></clipPath><g clip-path="url(#{id})">"#);
    }

    /// The SVG of the glyph and its box; `None` when nothing was painted.
    fn finish(self) -> Option<ColorGlyph> {
        let [x0, y0, x1, y1] = self.clip_box.or(self.bounds)?;
        let (w, h) = (x1 - x0, y1 - y0);
        if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
            return None;
        }
        let top = -y1;
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x0} {top} {w} {h}" width="{w}" height="{h}"><g transform="matrix(1 0 0 -1 0 0)">{}</g></svg>"#,
            self.body
        );
        (svg.len() <= MAX_SVG_BYTES).then_some(ColorGlyph::Svg {
            svg,
            bbox: [x0, y0, x1, y1],
        })
    }
}

impl<'a> Painter<'a> for SvgPainter<'a, '_> {
    fn outline_glyph(&mut self, glyph_id: GlyphId) {
        self.spend();
        self.path.clear();
        let rect = self
            .face
            .outline_glyph(glyph_id, &mut PathData { d: &mut self.path });
        self.path_bounds = rect.map(|r| {
            [
                f32::from(r.x_min),
                f32::from(r.y_min),
                f32::from(r.x_max),
                f32::from(r.y_max),
            ]
        });
        self.path_transform = self.transform;
    }

    fn paint(&mut self, paint: Paint<'a>) {
        self.spend();
        match paint {
            Paint::Solid(c) => {
                let fill = self.rgb(c);
                self.fill_path(&fill, f32::from(c.alpha) / 255.0);
            }
            Paint::LinearGradient(g) => {
                // The colour lines run parallel to p0p2: the gradient
                // goes from p0 to p1 projected onto its normal.
                let (dx, dy) = (g.x2 - g.x0, g.y2 - g.y0);
                let (nx, ny) = (dy, -dx);
                let len = nx * nx + ny * ny;
                let (x1, y1) = if len > 0.0 {
                    let k = ((g.x1 - g.x0) * nx + (g.y1 - g.y0) * ny) / len;
                    (g.x0 + k * nx, g.y0 + k * ny)
                } else {
                    (g.x1, g.y1)
                };
                let id = self.next_id('g');
                let _ = write!(
                    self.body,
                    r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{x1}" y2="{y1}" spreadMethod="{}""#,
                    g.x0,
                    g.y0,
                    Self::spread(g.extend)
                );
                if !self.gradient_transform() {
                    self.body.push_str("/>");
                    return;
                }
                self.body.push('>');
                self.stops(g.stops(0, &[]));
                self.body.push_str("</linearGradient>");
                self.fill_path(&format!("url(#{id})"), 1.0);
            }
            Paint::RadialGradient(g) => {
                let id = self.next_id('g');
                let _ = write!(
                    self.body,
                    r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{}" fx="{}" fy="{}" fr="{}" spreadMethod="{}""#,
                    g.x1,
                    g.y1,
                    g.r1,
                    g.x0,
                    g.y0,
                    g.r0,
                    Self::spread(g.extend)
                );
                if !self.gradient_transform() {
                    self.body.push_str("/>");
                    return;
                }
                self.body.push('>');
                self.stops(g.stops(0, &[]));
                self.body.push_str("</radialGradient>");
                self.fill_path(&format!("url(#{id})"), 1.0);
            }
            Paint::SweepGradient(g) => {
                let first = g
                    .stops(0, &[])
                    .min_by(|a, b| a.stop_offset.total_cmp(&b.stop_offset));
                if let Some(s) = first {
                    let c = s.color;
                    let fill = self.rgb(c);
                    self.fill_path(&fill, f32::from(c.alpha) / 255.0);
                }
            }
        }
    }

    fn push_clip(&mut self) {
        self.spend();
        let path = std::mem::take(&mut self.path);
        let m = self.path_transform;
        self.clip(&path, m);
        self.path = path;
    }

    fn push_clip_box(&mut self, clip: ClipBox) {
        self.spend();
        let (x0, y0, x1, y1) = (clip.x_min, clip.y_min, clip.x_max, clip.y_max);
        if self.clip_box.is_none() && self.transforms.is_empty() {
            self.clip_box = Some([x0, y0, x1, y1]);
        }
        let path = format!("M{x0} {y0}L{x1} {y0}L{x1} {y1}L{x0} {y1}Z");
        let m = self.transform;
        self.clip(&path, m);
    }

    fn pop_clip(&mut self) {
        self.spend();
        self.body.push_str("</g>");
    }

    fn push_layer(&mut self, _mode: CompositeMode) {
        self.spend();
        self.body.push_str("<g>");
    }

    fn pop_layer(&mut self) {
        self.spend();
        self.body.push_str("</g>");
    }

    fn push_transform(&mut self, transform: Transform) {
        self.spend();
        self.transforms.push(self.transform);
        self.transform = mul(self.transform, matrix(transform));
    }

    fn pop_transform(&mut self) {
        self.spend();
        if let Some(t) = self.transforms.pop() {
            self.transform = t;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{FontSet, FontStore};

    /// Runs `f` with the fixture fonts `EmojiTest-COLR`, `-CBDT`, `-sbix` and
    /// `limits/colr-fanout`, keyed 0 to 3.
    fn with_fonts(f: impl FnOnce(&FontSet<'_>)) {
        let mut store = FontStore::new();
        let files = [
            ("colr", "EmojiTest-COLR.ttf"),
            ("cbdt", "EmojiTest-CBDT.ttf"),
            ("sbix", "EmojiTest-sbix.ttf"),
            ("fanout", "limits/colr-fanout.ttf"),
        ];
        for (id, file) in files {
            let bytes = crate::test_fixtures::fixture(&format!("fonts/{file}"));
            store.register(id, bytes).unwrap();
        }
        let json = r#"{"version":1,"fonts":[{"id":"colr","family":"a"},{"id":"cbdt","family":"b"},
            {"id":"sbix","family":"c"},{"id":"fanout","family":"d"}],"pages":[]}"#;
        let doc: crate::schema::Document = serde_json::from_str(json).unwrap();
        f(&FontSet::load(&doc, &store).unwrap());
    }

    /// Glyph ids in `EmojiTest-COLR` (see `scripts/fixtures/emoji_fonts.py`).
    const KEYCAP_ONE: u16 = 14;
    const LAPTOP: u16 = 16;
    const WOMAN_LAPTOP: u16 = 17;

    fn svg_of(font: &LoadedFont<'_>, gid: u16) -> (String, [f32; 4]) {
        match font.color_glyph(gid) {
            Some(ColorGlyph::Svg { svg, bbox }) => (svg, bbox),
            other => panic!("glyph {gid}: {other:?}"),
        }
    }

    fn svg(font: &LoadedFont<'_>, c: char) -> (String, [f32; 4]) {
        svg_of(font, font.glyph(c).unwrap())
    }

    #[test]
    fn colr_v1_glyphs_become_svg_with_gradients_clips_and_transforms() {
        with_fonts(|fonts| {
            let colr = fonts.get(0);
            // A radial gradient and solid fills, in the font's clip box.
            let (grin, bbox) = svg(colr, '😀');
            assert_eq!(bbox, [50.0, -150.0, 1050.0, 850.0]);
            assert!(grin.contains(r#"viewBox="50 -850 1000 1000""#), "{grin}");
            assert!(grin.contains("<radialGradient"), "{grin}");
            assert!(grin.contains(r#"fill="rgb(60,40,30)""#), "{grin}");
            assert!(grin.contains(r#"fill-opacity="0.796"#), "{grin}");
            assert!(grin.contains("<clipPath"), "{grin}");
            // A linear gradient whose colour lines run along p0p2: from
            // (100, -100) to (1000, 800) with p2 straight right of p0, the
            // gradient runs straight up.
            let (key, bbox) = svg_of(colr, KEYCAP_ONE);
            assert!(
                key.contains(r#"x1="100" y1="-100" x2="100" y2="800""#),
                "{key}"
            );
            // No clip box: the bounds of what is filled.
            assert_eq!(bbox, [100.0, -100.0, 1000.0, 800.0]);
            // A sweep gradient draws with its first stop.
            let (woman, _) = svg(colr, '👩');
            assert!(woman.contains(r#"fill="rgb(90,30,140)""#), "{woman}");
            // A composite and a rotation.
            let (laptop, _) = svg_of(colr, LAPTOP);
            assert!(laptop.contains("<g>"), "{laptop}");
            assert!(laptop.contains("matrix(0.99"), "{laptop}");
            // Other colour glyphs painted again under transforms.
            let (coder, _) = svg_of(colr, WOMAN_LAPTOP);
            assert!(
                coder.contains("matrix(0.6000061 0 0 0.6000061 0 300)"),
                "{coder}"
            );
            assert!(coder.contains(r#"fill="rgb(150,155,165)""#), "{coder}");
        });
    }

    /// Layers in the foreground colour take the text colour; the others
    /// keep the palette's.
    #[test]
    fn foreground_layers_take_the_text_colour() {
        with_fonts(|fonts| {
            let colr = fonts.get(0);
            let star = colr.glyph('\u{2B50}').unwrap();
            let Some(ColorGlyph::Svg { svg: red, .. }) = colr.color_glyph_in(star, [255, 0, 0])
            else {
                panic!("no colour")
            };
            assert!(red.contains(r#"fill="rgb(255,0,0)""#), "{red}");
            assert!(red.contains(r#"fill="rgb(255,200,40)""#), "{red}");
            let (black, _) = svg_of(colr, star);
            assert!(black.contains(r#"fill="rgb(0,0,0)""#), "{black}");
            assert!(colr.color_glyph_tinted(star).unwrap().1);
            // Glyphs with no layer in the foreground colour, bitmaps.
            for c in ['😀', '\u{2764}', '👩'] {
                assert!(!colr.color_glyph_tinted(colr.glyph(c).unwrap()).unwrap().1);
            }
            for g in [WOMAN_LAPTOP, LAPTOP, KEYCAP_ONE] {
                assert!(!colr.color_glyph_tinted(g).unwrap().1);
            }
            let cbdt = fonts.get(1);
            assert!(
                !cbdt
                    .color_glyph_tinted(cbdt.glyph('😀').unwrap())
                    .unwrap()
                    .1
            );
        });
    }

    #[test]
    fn colr_v0_layers_and_ligatures() {
        with_fonts(|fonts| {
            let colr = fonts.get(0);
            let (heart, bbox) = svg(colr, '\u{2764}');
            assert_eq!(heart.matches(r#"fill="rgb(220,30,60)""#).count(), 3);
            assert!(bbox[0] >= 50.0 && bbox[2] <= 1050.0, "{bbox:?}");
            // Glyphs without colour: the digit, the space.
            assert_eq!(colr.color_glyph(colr.glyph('1').unwrap()), None);
            assert_eq!(colr.color_glyph(colr.glyph(' ').unwrap()), None);
        });
    }

    #[test]
    fn bitmaps_come_from_cbdt_and_sbix() {
        with_fonts(|fonts| {
            for key in [1, 2] {
                let font = fonts.get(key);
                let Some(ColorGlyph::Png { png, bbox }) =
                    font.color_glyph(font.glyph('😀').unwrap())
                else {
                    panic!("no bitmap in {}", font.id);
                };
                assert!(png.starts_with(b"\x89PNG"));
                // 64 px at 64 ppem in a 1000-unit em, from (3, -10) px: the
                // largest strike, also in the sbix font, whose first strike
                // is 32 ppem.
                assert_eq!(&png[16..20], 64u32.to_be_bytes(), "{}", font.id);
                assert_eq!(bbox, [46.875, -156.25, 1046.875, 843.75], "{}", font.id);
                assert_eq!(font.color_glyph(font.glyph(' ').unwrap()), None);
            }
        });
    }

    #[test]
    fn a_glyph_over_the_work_budget_has_no_colour() {
        with_fonts(|fonts| {
            let fanout = fonts.get(3);
            let t = std::time::Instant::now();
            assert_eq!(fanout.color_glyph(fanout.glyph('😀').unwrap()), None);
            assert!(t.elapsed().as_secs() < 2, "took {:?}", t.elapsed());
        });
    }

    #[test]
    fn matrices_invert() {
        let m = [2.0, 0.5, -1.0, 3.0, 10.0, -4.0];
        let i = invert(m).unwrap();
        let p = apply(i, apply(m, (7.0, -3.0)));
        assert!(
            (p.0 - 7.0).abs() < 1e-4 && (p.1 + 3.0).abs() < 1e-4,
            "{p:?}"
        );
        assert_eq!(invert([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]), None);
    }
}
