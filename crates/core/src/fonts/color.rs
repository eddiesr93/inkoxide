//! Colour glyphs: `COLR` layers (versions 0 and 1) written as SVG, the
//! SVG documents of an `SVG ` table, and PNG bitmaps from `CBDT` or
//! `sbix`. The text painter draws them as images in place of the glyphs.
//!
//! The SVG keeps what the PDF can show: outlines filled with solid
//! colours and linear and radial gradients, clips, and transforms. A
//! sweep gradient, which SVG and PDF lack, is drawn as wedges of solid
//! colour, a degree or less each. Composites blend with the PDF's blend
//! modes, and the Porter-Duff operators are built from alpha masks; `plus`
//! draws as `screen`, the nearest of the PDF's. Fonts are untrusted, and
//! `COLR` v1 glyphs nest
//! (a layer can paint another colour glyph, whose layers paint others),
//! so the work per glyph is bounded: a glyph whose paint graph reaches
//! more than [`MAX_PAINT_OPS`] paint tables (each counted as often as it
//! is reached), or that takes more than [`MAX_PAINT_OPS`] painter calls or
//! [`MAX_SVG_BYTES`] of markup, has no colour.
//!
//! Layers in the foreground colour (palette index 0xFFFF) take the text
//! colour, as in browsers: such a glyph is painted once per text colour
//! it is drawn in. So do the parts of an SVG glyph in `currentColor`,
//! `context-fill` or `context-stroke`.
//!
//! An SVG glyph is drawn from its document without the document's other
//! glyphs, in font units with y down from the baseline, the root
//! `viewBox` fitted to the em square, as the `SVG ` table says. Its
//! document is parsed with the limits of an SVG image; a compressed one is
//! inflated to at most [`MAX_SVG_BYTES`].

use std::collections::HashMap;
use std::fmt::Write;

use read_fonts::model::glyph::bitmap::Pixels;
use read_fonts::model::glyph::color::{Brush, ColorStop, CompositeMode, Extend, PaintSink};
use read_fonts::model::glyph::outline::{DrawMemory, DrawOptions, OutlineGlyphs, PathSink};
use read_fonts::model::{self, glyph::color::Color};
use read_fonts::types::{BoundingBox, GlyphId, GlyphId16, Matrix};
use read_fonts::{FontRef, TableProvider};

use super::LoadedFont;
use crate::geom::{apply, invert, mul};

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
    /// colour black), else its SVG document, else its largest PNG bitmap. `None` for a glyph
    /// without colour, or whose colour data is malformed or over the
    /// limits.
    #[cfg(test)]
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
        let id = GlyphId::from(gid);
        if let Some(glyph) = self.font.color_glyphs().get(id) {
            let colr = self.tables.table_data(read_fonts::types::Tag::new(b"COLR"));
            if colr.is_some_and(|colr| !paint_graph_within(colr.as_bytes(), gid, MAX_PAINT_OPS)) {
                return None;
            }
            return colr_svg(&self.font, &glyph, text);
        }
        if let Some((doc, alone)) = svg_document(&self.tables, gid) {
            if let Some(glyph) = svg_table_glyph(doc, alone, gid, text, self.metrics().units_per_em)
            {
                return Some(glyph);
            }
        }
        // The largest strike that has the glyph.
        let img = self.font.bitmap_strikes().select_bitmap(id, None)?;
        let Pixels::Png(png) = img.pixels() else {
            return None;
        };
        let (width, height) = (img.width() as f32, img.height() as f32);
        if width == 0.0 || height == 0.0 {
            return None;
        }
        // Each corner from where the table puts the image, in pixels, its
        // outer bearings aside.
        let upem = self.metrics().units_per_em;
        let (sx, sy) = (
            upem / f32::from(img.ppem_x()),
            upem / f32::from(img.ppem_y()),
        );
        let left = img.inner_x_bearing() as f32;
        let bottom = match img.origin() {
            model::glyph::bitmap::Origin::TopLeft => img.inner_y_bearing() as f32 - height,
            model::glyph::bitmap::Origin::BottomLeft => img.inner_y_bearing() as f32,
        };
        let png = ColorGlyph::Png {
            png: png.to_vec(),
            bbox: [
                left * sx,
                bottom * sy,
                (left + width) * sx,
                (bottom + height) * sy,
            ],
        };
        Some((png, false))
    }
}

/// How deep read-fonts follows nested paint tables.
const MAX_PAINT_DEPTH: usize = 64;

/// Whether painting glyph `gid` from `COLR` table `colr` reaches at most
/// `limit` paint tables, each counted as often as it is reached. Painting
/// visits every one of them, whether or not it calls the painter, so this
/// bounds the work before it starts. A cycle, nesting deeper than
/// read-fonts follows, or more than `limit` is over. Each table is
/// counted once and its count reused, so this takes time in proportion to
/// the table, however much the graph fans out.
fn paint_graph_within(colr: &[u8], gid: u16, limit: usize) -> bool {
    PaintGraph::new(colr, limit).is_some_and(|mut g| g.glyph(gid).is_some())
}

/// The paint graph of a `COLR` table, for [`paint_graph_within`]. Offsets
/// are from the start of the table, as read-fonts takes them.
struct PaintGraph<'a> {
    data: &'a [u8],
    limit: usize,
    /// Version 0 base glyph records: offset and count.
    v0: (usize, usize),
    /// Version 1 base glyph list: offset and count.
    v1: (usize, usize),
    /// Layer list: offset and count.
    layers: (usize, usize),
    /// Paint tables counted, by offset.
    counted: HashMap<usize, usize>,
    /// Paint tables being counted, outermost first.
    stack: Vec<usize>,
}

impl<'a> PaintGraph<'a> {
    fn new(data: &'a [u8], limit: usize) -> Option<Self> {
        let r = Reader(data);
        let version = r.u16(0)?;
        let v0 = (r.u32(4)?, usize::from(r.u16(2)?));
        let (mut v1, mut layers) = ((0, 0), (0, 0));
        if version >= 1 {
            let list = r.u32(14)?;
            v1 = (list, r.u32(list)?);
            let layer_list = r.u32(18)?;
            if layer_list != 0 {
                layers = (layer_list, r.u32(layer_list)?);
            }
        }
        Some(Self {
            data,
            limit,
            v0,
            v1,
            layers,
            counted: HashMap::new(),
            stack: Vec::new(),
        })
    }

    /// The paint tables painting glyph `gid` reaches, as read-fonts paints
    /// it: its version 1 paint, else its version 0 layers. `None` when
    /// over.
    fn glyph(&mut self, gid: u16) -> Option<usize> {
        let r = Reader(self.data);
        let find = |(at, count): (usize, usize), size: usize| {
            let (mut lo, mut hi) = (0, count);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let record = at.checked_add(mid.checked_mul(size)?)?;
                match r.u16(record)?.cmp(&gid) {
                    std::cmp::Ordering::Less => lo = mid + 1,
                    std::cmp::Ordering::Greater => hi = mid,
                    std::cmp::Ordering::Equal => return Some(record),
                }
            }
            None
        };
        if let Some(record) = self
            .v1
            .0
            .checked_add(4)
            .and_then(|at| find((at, self.v1.1), 6))
        {
            let paint = r
                .u32(record.saturating_add(2))
                .and_then(|o| self.v1.0.checked_add(o));
            return paint.map_or(Some(0), |p| self.paint(p));
        }
        if let Some(record) = find(self.v0, 6) {
            let layers = usize::from(r.u16(record.saturating_add(4)).unwrap_or(0));
            return (layers <= self.limit).then_some(layers);
        }
        Some(0)
    }

    /// The paint tables reached from the one at `at`, itself included.
    fn paint(&mut self, at: usize) -> Option<usize> {
        if let Some(&n) = self.counted.get(&at) {
            return Some(n);
        }
        if self.stack.contains(&at) || self.stack.len() >= MAX_PAINT_DEPTH {
            return None;
        }
        self.stack.push(at);
        let below = self.children(at);
        self.stack.pop();
        let n = below?.checked_add(1).filter(|&n| n <= self.limit)?;
        self.counted.insert(at, n);
        Some(n)
    }

    /// The paint tables reached from the children of the one at `at`.
    fn children(&mut self, at: usize) -> Option<usize> {
        let r = Reader(self.data);
        let child = |field: usize| {
            r.u24(at.saturating_add(field))
                .and_then(|o| at.checked_add(o))
        };
        let Some(format) = r.u8(at) else {
            return Some(0);
        };
        let mut total = 0usize;
        let mut add = |g: &mut Self, n: Option<usize>| -> Option<()> {
            total = total.checked_add(n?).filter(|&t| t <= g.limit)?;
            Some(())
        };
        match format {
            // PaintColrLayers: the layers it names, while they can be read.
            1 => {
                let (Some(count), Some(first)) =
                    (r.u8(at.saturating_add(1)), r.u32(at.saturating_add(2)))
                else {
                    return Some(0);
                };
                for i in 0..usize::from(count) {
                    let Some(index) = first.checked_add(i).filter(|&i| i < self.layers.1) else {
                        break;
                    };
                    let entry = index
                        .saturating_mul(4)
                        .saturating_add(self.layers.0.saturating_add(4));
                    let Some(paint) = r.u32(entry).and_then(|o| self.layers.0.checked_add(o))
                    else {
                        break;
                    };
                    let n = self.paint(paint);
                    add(self, n)?;
                }
            }
            // PaintGlyph, and the transforms: one child, first.
            10 | 12..=31 => {
                if let Some(c) = child(1) {
                    let n = self.paint(c);
                    add(self, n)?;
                }
            }
            // PaintColrGlyph: another glyph.
            11 => {
                if let Some(gid) = r.u16(at.saturating_add(1)) {
                    let n = self.glyph(gid);
                    add(self, n)?;
                }
            }
            // PaintComposite: source and backdrop.
            32 => {
                for field in [1, 5] {
                    if let Some(c) = child(field) {
                        let n = self.paint(c);
                        add(self, n)?;
                    }
                }
            }
            // Solid fills and gradients.
            _ => {}
        }
        Some(total)
    }
}

/// Big-endian reads that fail past the end.
#[derive(Clone, Copy)]
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes<const N: usize>(self, at: usize) -> Option<[u8; N]> {
        self.0.get(at..at.checked_add(N)?)?.try_into().ok()
    }
    fn u8(self, at: usize) -> Option<u8> {
        self.0.get(at).copied()
    }
    fn u16(self, at: usize) -> Option<u16> {
        self.bytes(at).map(u16::from_be_bytes)
    }
    fn u24(self, at: usize) -> Option<usize> {
        let [a, b, c] = self.bytes(at)?;
        Some(usize::from(a) << 16 | usize::from(b) << 8 | usize::from(c))
    }
    fn u32(self, at: usize) -> Option<usize> {
        self.bytes(at).map(|b| u32::from_be_bytes(b) as usize)
    }
}

/// The text of an `SVG ` table document, inflated when compressed; `None`
/// over [`MAX_SVG_BYTES`].
fn svg_document_text(data: &[u8]) -> Option<std::borrow::Cow<'_, [u8]>> {
    use std::io::Read;
    if !data.starts_with(&[0x1F, 0x8B]) {
        return (data.len() <= MAX_SVG_BYTES).then_some(std::borrow::Cow::Borrowed(data));
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data)
        .take(MAX_SVG_BYTES as u64 + 1)
        .read_to_end(&mut out)
        .ok()?;
    (out.len() <= MAX_SVG_BYTES).then_some(std::borrow::Cow::Owned(out))
}

/// Whether `n` is an element of another glyph than `id` (`glyph` and a
/// number).
fn other_glyph(n: usvg::roxmltree::Node<'_, '_>, id: &str) -> bool {
    n.is_element()
        && n.attribute("id").is_some_and(|i| {
            i != id
                && i.strip_prefix("glyph")
                    .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
}

/// The document of the `SVG ` table that draws glyph `gid`, and whether it
/// is that glyph's alone.
fn svg_document<'a>(tables: &FontRef<'a>, gid: u16) -> Option<(&'a [u8], bool)> {
    let list = tables.svg().ok()?.svg_document_list().ok()?;
    let records = list.document_records();
    let gid = GlyphId16::new(gid);
    let at = records
        .binary_search_by(|r| {
            if r.start_glyph_id() > gid {
                std::cmp::Ordering::Greater
            } else if r.end_glyph_id() < gid {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()?;
    let r = records.get(at)?;
    let start = r.svg_doc_offset() as usize;
    let end = start.checked_add(r.svg_doc_length() as usize)?;
    let data = list.offset_data().as_bytes().get(start..end)?;
    Some((data, r.start_glyph_id() == r.end_glyph_id()))
}

/// Glyph `gid` from `doc`, a document of the `SVG ` table (the glyph's
/// `alone`, or several glyphs'), in a font of `upem` units per em, its
/// parts in the text colour in `text`, and whether it has such parts.
/// `None` when the document has no element for the glyph (and is not the
/// glyph's alone), cannot be parsed or drawn, or is over the limits.
fn svg_table_glyph(
    doc: &[u8],
    alone: bool,
    gid: u16,
    text: TextColor,
    upem: f32,
) -> Option<(ColorGlyph, bool)> {
    use crate::images::svg::{ViewBox, SVG_NS, XLINK_NS};
    let data = svg_document_text(doc)?;
    let xml = crate::images::svg::parse_document(&data).ok()?;
    let source = xml.input_text();
    let root = xml.root_element();
    let id = format!("glyph{gid}");
    let target = xml
        .descendants()
        .find(|n| n.attribute("id") == Some(id.as_str()));
    let mut content = String::new();
    match target {
        // Deep in the document: drawn by reference, the rest only defined.
        Some(n) if n != root && n.parent() != Some(root) => {
            let _ = write!(
                content,
                r##"<defs>{}</defs><use href="#{id}"/>"##,
                &source[root.range()]
            );
        }
        // The root's content, without the other glyphs.
        Some(_) => {}
        None if alone => {}
        None => return None,
    }
    if content.is_empty() {
        for child in root.children().filter(|&c| !other_glyph(c, &id)) {
            content.push_str(&source[child.range()]);
        }
    }
    let lower = content.to_ascii_lowercase();
    let tinted = ["currentcolor", "context-fill", "context-stroke"]
        .iter()
        .any(|k| lower.contains(k));
    let mut attrs = String::new();
    if tinted {
        let [r, g, b] = text;
        let _ = write!(attrs, r#" color="rgb({r},{g},{b})""#);
        content = content
            .replace("context-fill", "currentColor")
            .replace("context-stroke", "currentColor");
    }
    // The root `viewBox` fits the em square.
    if let Some(vb) = ViewBox::of(root) {
        let [vx, vy, vw, vh] = vb.rect;
        let (fx, fy, fw, fh) = vb.fit((0.0, 0.0, upem, upem));
        let (sx, sy) = (fw / vw, fh / vh);
        let (e, f) = (fx - vx * sx, fy - vy * sy);
        let _ = write!(attrs, r#" transform="matrix({sx} 0 0 {sy} {e} {f})""#);
    }
    let svg = |[x, y, w, h]: [f32; 4]| {
        format!(
            r#"<svg xmlns="{SVG_NS}" xmlns:xlink="{XLINK_NS}" viewBox="{x} {y} {w} {h}" width="{w}" height="{h}"><g{attrs}>{content}</g></svg>"#
        )
    };
    // Its box, from what it draws.
    let (tree, _) = crate::images::svg::parse(svg([0.0, 0.0, 1.0, 1.0]).as_bytes()).ok()?;
    let b = tree.root().abs_stroke_bounding_box();
    let (x, y, w, h) = (b.x(), b.y(), b.width(), b.height());
    if !([x, y, w, h].iter().all(|v| v.is_finite()) && w > 0.0 && h > 0.0) {
        return None;
    }
    let svg = svg([x, y, w, h]);
    (svg.len() <= MAX_SVG_BYTES).then_some((
        ColorGlyph::Svg {
            svg,
            bbox: [x, -(y + h), x + w, -y],
        },
        tinted,
    ))
}

/// Colour glyph `glyph` of `font` as SVG, its layers in the foreground
/// colour in `text`, and whether anything was painted in `text`.
fn colr_svg(
    font: &model::Font,
    glyph: &model::glyph::color::ColorGlyph<'_>,
    text: TextColor,
) -> Option<(ColorGlyph, bool)> {
    let mut painter = SvgPainter::new(font, text);
    glyph.paint(&mut painter).ok()?;
    let seen = painter.text_seen;
    painter.finish().map(|glyph| (glyph, seen))
}

/// The 2×3 matrix of `m`, `[a b c d e f]`.
fn matrix(m: Matrix<f32>) -> [f32; 6] {
    [m.xx, m.yx, m.xy, m.yy, m.dx, m.dy]
}

/// A colour, `[r, g, b, a]`.
type Rgba = [u8; 4];

/// Writes glyph outlines as SVG path data, and the box of their points.
struct PathData<'s> {
    d: &'s mut String,
    bounds: Option<[f32; 4]>,
}

impl PathData<'_> {
    fn point(&mut self, x: f32, y: f32) {
        self.bounds = Some(match self.bounds {
            Some([a, b, c, d]) => [a.min(x), b.min(y), c.max(x), d.max(y)],
            None => [x, y, x, y],
        });
    }
}

impl PathSink for PathData<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
        let _ = write!(self.d, "M{x} {y}");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
        let _ = write!(self.d, "L{x} {y}");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.point(x1, y1);
        self.point(x, y);
        let _ = write!(self.d, "Q{x1} {y1} {x} {y}");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.point(x1, y1);
        self.point(x2, y2);
        self.point(x, y);
        let _ = write!(self.d, "C{x1} {y1} {x2} {y2} {x} {y}");
    }
    fn close(&mut self) {
        self.d.push('Z');
    }
}

/// A `COLR` painter writing SVG in font units, y up.
struct SvgPainter<'f> {
    outlines: OutlineGlyphs<'f>,
    /// The colours of the font's first palette.
    palette: &'f [Color],
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
    /// Past the budget: read-fonts cannot stop a painter, so the calls
    /// after that do nothing (the paint graph's size bounds them), and the
    /// glyph has no colour.
    over: bool,
    /// A colour past the palette was asked for: the glyph has no colour,
    /// and draws as its outline.
    missing_color: bool,
    /// The text colour, and whether a fill or a stop was in it.
    text: TextColor,
    text_seen: bool,
    /// The layers open, innermost last: where each starts in `body`, and
    /// how it composites.
    layers: Vec<(usize, CompositeMode)>,
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Wedges a sweep gradient is sampled in: one a degree.
const SWEEP_STEPS: usize = 360;

/// The region of a composite's masks, in font units: wider than any glyph.
const MASK_REGION: &str =
    r#"maskUnits="userSpaceOnUse" x="-100000" y="-100000" width="200000" height="200000""#;
const MASK_RECT: &str = r#"x="-100000" y="-100000" width="200000" height="200000""#;

/// The colour of a colour line, `[r, g, b, a]` from 0 to 1, at `t` (0 at
/// the first stop's offset of the gradient's range, 1 at its end), extended
/// past its stops as `extend` says. `stops`, offsets and colours, are
/// sorted and not empty.
fn sweep_color(stops: &[(f32, Rgba)], extend: Extend, t: f32) -> [f32; 4] {
    let rgba = |c: Rgba| c.map(|v| f32::from(v) / 255.0);
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    let (o0, o1) = (first.0, last.0);
    let t = if t.is_nan() { 0.0 } else { t };
    let t = match extend {
        Extend::Repeat | Extend::Reflect if o1 <= o0 => o0,
        Extend::Repeat => o0 + (t - o0).rem_euclid(o1 - o0),
        Extend::Reflect => {
            let u = ((t - o0) / (o1 - o0)).rem_euclid(2.0);
            o0 + (o1 - o0) * if u > 1.0 { 2.0 - u } else { u }
        }
        _ => t,
    };
    if t <= o0 {
        return rgba(first.1);
    }
    if t >= o1 {
        return rgba(last.1);
    }
    let k = stops.partition_point(|s| s.0 <= t);
    let (a, b) = (stops[k - 1], stops[k]);
    let span = b.0 - a.0;
    let f = if span > 0.0 { (t - a.0) / span } else { 1.0 };
    let (ca, cb) = (rgba(a.1), rgba(b.1));
    // Rounded to the 8 bits colours have, so that equal wedges merge.
    [0, 1, 2, 3].map(|i| ((ca[i] + (cb[i] - ca[i]) * f) * 255.0).round() / 255.0)
}

impl<'f> SvgPainter<'f> {
    fn new(font: &'f model::Font, text: TextColor) -> Self {
        Self {
            outlines: font.outline_glyphs(),
            palette: font.palettes().get(0).map_or(&[], |p| p.colors()),
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
            over: false,
            missing_color: false,
            text,
            text_seen: false,
            layers: Vec::new(),
        }
    }

    /// The colour at `index` of the palette, its alpha times `alpha`: the
    /// text colour at the foreground index (0xFFFF). Past the palette, it
    /// notes the glyph has no colour.
    fn color(&mut self, index: u16, alpha: f32) -> Rgba {
        let [r, g, b, a] = if index == 0xFFFF {
            let [r, g, b] = self.text;
            [r, g, b, 255]
        } else if let Some(c) = self.palette.get(usize::from(index)) {
            [c.red, c.green, c.blue, c.alpha]
        } else {
            self.missing_color = true;
            [0; 4]
        };
        let alpha = if alpha.is_nan() {
            0.0
        } else {
            alpha.clamp(0.0, 1.0)
        };
        [r, g, b, (f32::from(a) * alpha).round() as u8]
    }

    /// `c` as an SVG colour, noting whether it is the text colour.
    fn rgb(&mut self, [r, g, b, _]: Rgba) -> String {
        self.text_seen |= [r, g, b] == self.text;
        format!("rgb({r},{g},{b})")
    }

    /// Counts one call against the budget; false past it.
    fn spend(&mut self) -> bool {
        self.ops += 1;
        self.over |= self.ops > MAX_PAINT_OPS || self.body.len() > MAX_SVG_BYTES;
        !self.over
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

    /// Makes glyph `glyph`'s outline, under the transform in effect, the
    /// last outline.
    fn outline(&mut self, glyph: GlyphId) {
        self.path.clear();
        let mut data = PathData {
            d: &mut self.path,
            bounds: None,
        };
        let drawn = self.outlines.get(glyph).is_some_and(|o| {
            o.draw(&DrawOptions::default(), DrawMemory::Scratch, &mut data)
                .is_ok()
        });
        self.path_bounds = if drawn { data.bounds } else { None };
        if !drawn {
            self.path.clear();
        }
        self.path_transform = self.transform;
    }

    /// Fills the last outline with `fill` (a colour or `url(#id)`).
    fn fill_path(&mut self, fill: &str, opacity: f32) {
        if self.path.is_empty() {
            return;
        }
        self.add_path_bounds();
        let _ = write!(self.body, r#"<path d="{}" fill="{fill}""#, self.path);
        if opacity < 1.0 {
            let _ = write!(self.body, r#" fill-opacity="{opacity}""#);
        }
        let m = self.path_transform;
        self.transform_attr("transform", m);
        self.body.push_str("/>");
    }

    /// Adds the last outline's box, under its transform, to `bounds`.
    fn add_path_bounds(&mut self) {
        for (x, y) in self.path_corners() {
            self.bounds = Some(match self.bounds {
                Some([a, b, c, d]) => [a.min(x), b.min(y), c.max(x), d.max(y)],
                None => [x, y, x, y],
            });
        }
    }

    /// The corners of the last outline's box, in glyph space.
    fn path_corners(&self) -> Vec<(f32, f32)> {
        let Some([x0, y0, x1, y1]) = self.path_bounds else {
            return Vec::new();
        };
        let m = self.path_transform;
        [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
            .into_iter()
            .map(|c| apply(m, c))
            .collect()
    }

    /// Fills the last outline with `brush`, its coordinates in paint space
    /// (the transform in effect).
    fn fill_brush(&mut self, brush: &Brush<'_>) {
        match *brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => {
                let c = self.color(palette_index, alpha);
                let fill = self.rgb(c);
                self.fill_path(&fill, f32::from(c[3]) / 255.0);
            }
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let id = self.next_id('g');
                let _ = write!(
                    self.body,
                    r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}" spreadMethod="{}""#,
                    p0.x,
                    p0.y,
                    p1.x,
                    p1.y,
                    Self::spread(extend)
                );
                if !self.gradient_transform() {
                    self.body.push_str("/>");
                    return;
                }
                self.body.push('>');
                self.stops(color_stops);
                self.body.push_str("</linearGradient>");
                self.fill_path(&format!("url(#{id})"), 1.0);
            }
            // Normalising the stops can give a circle a negative radius,
            // which SVG has not: it starts at a point.
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let id = self.next_id('g');
                let _ = write!(
                    self.body,
                    r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{}" fx="{}" fy="{}" fr="{}" spreadMethod="{}""#,
                    c1.x,
                    c1.y,
                    r1.max(0.0),
                    c0.x,
                    c0.y,
                    r0.max(0.0),
                    Self::spread(extend)
                );
                if !self.gradient_transform() {
                    self.body.push_str("/>");
                    return;
                }
                self.body.push('>');
                self.stops(color_stops);
                self.body.push_str("</radialGradient>");
                self.fill_path(&format!("url(#{id})"), 1.0);
            }
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => self.sweep((c0.x, c0.y), (start_angle, end_angle), color_stops, extend),
        }
    }

    /// Fills the last outline with a sweep gradient around `centre`: wedges
    /// around it, in paint space, each in the colour at its middle, clipped
    /// to the outline. Wedges of one colour merge. An opaque wedge reaches a
    /// degree into the next, which is drawn over it, and starts a little
    /// behind the centre, so that no seam shows between them. read-fonts
    /// gives the `angles`, in degrees, clockwise.
    fn sweep(
        &mut self,
        (cx, cy): (f32, f32),
        (a0, a1): (f32, f32),
        stops: &[ColorStop],
        extend: Extend,
    ) {
        if self.path.is_empty() || stops.is_empty() {
            return;
        }
        let mut stops: Vec<(f32, Rgba)> = stops
            .iter()
            .map(|s| (s.offset, self.color(s.palette_index, s.alpha)))
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        for &(_, [r, g, b, _]) in &stops {
            self.text_seen |= [r, g, b] == self.text;
        }
        // Far enough from the centre to cover the outline, in paint space.
        let Some(inverse) = invert(self.transform) else {
            return;
        };
        let reach = self
            .path_corners()
            .into_iter()
            .map(|p| {
                let (x, y) = apply(inverse, p);
                (x - cx).hypot(y - cy)
            })
            .fold(0.0f32, f32::max);
        if !(reach.is_finite() && reach > 0.0) {
            return;
        }
        let radius = reach * 1.01 + 1.0;
        // At an angle counter-clockwise in the font's y-up space, as the
        // wedges are drawn: clockwise, the rest of the turn.
        let color_at = |angle: f32| {
            let clockwise = 360.0 - angle;
            let t = if a0 == a1 {
                // No range: the first stop before it, the last after.
                if clockwise < a0 {
                    0.0
                } else {
                    1.0
                }
            } else {
                (clockwise - a0) / (a1 - a0)
            };
            sweep_color(&stops, extend, t)
        };
        let mut wedges: Vec<(f32, f32, [f32; 4])> = Vec::new();
        for step in 0..SWEEP_STEPS {
            let (from, to) = (step as f32, step as f32 + 1.0);
            let c = color_at(from + 0.5);
            match wedges.last_mut() {
                Some(last) if last.2 == c => last.1 = to,
                _ => wedges.push((from, to, c)),
            }
        }
        self.add_path_bounds();
        let path = std::mem::take(&mut self.path);
        let m = self.path_transform;
        self.clip(&path, m);
        self.path = path;
        let mut out = String::new();
        let back = reach * 0.003;
        for (k, (from, to, [r, gr, b, a])) in wedges.into_iter().enumerate() {
            if a <= 0.0 {
                continue;
            }
            let (mut from, mut to, mut apex) = (from, to, (cx, cy));
            if a >= 1.0 {
                if k == 0 {
                    from -= 1.0;
                }
                to = (to + 1.0).min(360.0);
                let mid = f32::midpoint(from, to).to_radians();
                apex = (cx - back * mid.cos(), cy - back * mid.sin());
            }
            let _ = write!(out, "<path d=\"M{} {}", apex.0, apex.1);
            // A point every 45° at most keeps the polygon around the arc.
            let n = ((to - from) / 45.0).ceil().max(1.0) as usize;
            for k in 0..=n {
                let t = (from + (to - from) * k as f32 / n as f32).to_radians();
                let _ = write!(out, "L{} {}", cx + radius * t.cos(), cy + radius * t.sin());
            }
            let rgb = format!(
                "rgb({},{},{})",
                (r * 255.0).round(),
                (gr * 255.0).round(),
                (b * 255.0).round()
            );
            let _ = write!(out, "Z\" fill=\"{rgb}\"");
            if a < 1.0 {
                let _ = write!(out, " fill-opacity=\"{a}\"");
            }
            self.body.push_str(&out);
            out.clear();
            let t = self.transform;
            self.transform_attr("transform", t);
            self.body.push_str("/>");
        }
        self.body.push_str("</g>");
    }

    /// Composites `source` onto `backdrop` (markup in the same space) as
    /// `mode` says. Each is defined once and drawn by reference, however
    /// often the operator uses it.
    fn composite(&mut self, mode: CompositeMode, backdrop: &str, source: &str) -> String {
        use CompositeMode as M;
        let (b, s) = (self.next_id('b'), self.next_id('s'));
        let mut out = format!(r#"<defs><g id="{b}">{backdrop}</g><g id="{s}">{source}</g></defs>"#);
        let draw = |id: &str| format!(r##"<use href="#{id}"/>"##);
        // `what` where `shape` is (alpha), or where it is not.
        let masked = |this: &mut Self, out: &mut String, what: &str, shape: &str, inside: bool| {
            let m = this.next_id('m');
            let _ = write!(
                out,
                r#"<mask id="{m}" mask-type="alpha" {MASK_REGION}>{}</mask>"#,
                draw(shape)
            );
            let m = if inside {
                m
            } else {
                // White everywhere, black where the shape is: a luminance
                // mask of what it leaves.
                let n = this.next_id('m');
                let _ = write!(
                    out,
                    r#"<mask id="{n}" {MASK_REGION}><rect {MASK_RECT} fill="white"/><g mask="url(#{m})"><rect {MASK_RECT} fill="black"/></g></mask>"#
                );
                n
            };
            let _ = write!(out, r#"<g mask="url(#{m})">{}</g>"#, draw(what));
        };
        let blend = |name: &str| {
            format!(
                r#"<g style="isolation:isolate">{}<g style="mix-blend-mode:{name}">{}</g></g>"#,
                draw(&b),
                draw(&s)
            )
        };
        match mode {
            M::Clear => return String::new(),
            M::Src => out.push_str(&draw(&s)),
            M::Dest => out.push_str(&draw(&b)),
            M::DestOver => {
                out.push_str(&draw(&s));
                out.push_str(&draw(&b));
            }
            M::SrcIn => masked(self, &mut out, &s, &b, true),
            M::DestIn => masked(self, &mut out, &b, &s, true),
            M::SrcOut => masked(self, &mut out, &s, &b, false),
            M::DestOut => masked(self, &mut out, &b, &s, false),
            M::SrcAtop => {
                out.push_str(&draw(&b));
                masked(self, &mut out, &s, &b, true);
            }
            M::DestAtop => {
                masked(self, &mut out, &s, &b, false);
                masked(self, &mut out, &b, &s, true);
            }
            M::Xor => {
                masked(self, &mut out, &s, &b, false);
                masked(self, &mut out, &b, &s, false);
            }
            M::Plus | M::Screen => out.push_str(&blend("screen")),
            M::Overlay => out.push_str(&blend("overlay")),
            M::Darken => out.push_str(&blend("darken")),
            M::Lighten => out.push_str(&blend("lighten")),
            M::ColorDodge => out.push_str(&blend("color-dodge")),
            M::ColorBurn => out.push_str(&blend("color-burn")),
            M::HardLight => out.push_str(&blend("hard-light")),
            M::SoftLight => out.push_str(&blend("soft-light")),
            M::Difference => out.push_str(&blend("difference")),
            M::Exclusion => out.push_str(&blend("exclusion")),
            M::Multiply => out.push_str(&blend("multiply")),
            M::HslHue => out.push_str(&blend("hue")),
            M::HslSaturation => out.push_str(&blend("saturation")),
            M::HslColor => out.push_str(&blend("color")),
            M::HslLuminosity => out.push_str(&blend("luminosity")),
            // Source over, and modes the table does not define.
            _ => {
                out.push_str(&draw(&b));
                out.push_str(&draw(&s));
            }
        }
        out
    }

    fn stops(&mut self, stops: &[ColorStop]) {
        let mut stops: Vec<_> = stops.to_vec();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        for s in stops {
            let c = self.color(s.palette_index, s.alpha);
            let color = self.rgb(c);
            let _ = write!(
                self.body,
                r#"<stop offset="{}" stop-color="{color}" stop-opacity="{}"/>"#,
                s.offset,
                f32::from(c[3]) / 255.0
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

    fn spread(extend: Extend) -> &'static str {
        match extend {
            Extend::Repeat => "repeat",
            Extend::Reflect => "reflect",
            _ => "pad",
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
        if self.over || self.missing_color {
            return None;
        }
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

impl PaintSink for SvgPainter<'_> {
    fn push_transform(&mut self, transform: Matrix<f32>) {
        if !self.spend() {
            return;
        }
        self.transforms.push(self.transform);
        self.transform = mul(self.transform, matrix(transform));
    }

    fn pop_transform(&mut self) {
        if !self.spend() {
            return;
        }
        if let Some(t) = self.transforms.pop() {
            self.transform = t;
        }
    }

    fn push_clip_glyph(&mut self, glyph: GlyphId) {
        if !self.spend() {
            return;
        }
        self.outline(glyph);
        let path = std::mem::take(&mut self.path);
        let m = self.path_transform;
        self.clip(&path, m);
        self.path = path;
    }

    fn push_clip_box(&mut self, clip: BoundingBox<f32>) {
        if !self.spend() {
            return;
        }
        let (x0, y0, x1, y1) = (clip.x_min, clip.y_min, clip.x_max, clip.y_max);
        if self.clip_box.is_none() && self.transforms.is_empty() {
            self.clip_box = Some([x0, y0, x1, y1]);
        }
        let path = format!("M{x0} {y0}L{x1} {y0}L{x1} {y1}L{x0} {y1}Z");
        let m = self.transform;
        self.clip(&path, m);
    }

    fn pop_clip(&mut self) {
        if !self.spend() {
            return;
        }
        self.body.push_str("</g>");
    }

    /// Fills the last outline: the glyph clipped to last, as in the
    /// `COLR` paint graph a fill is under the glyph it fills.
    fn fill(&mut self, brush: Brush<'_>) {
        if !self.spend() {
            return;
        }
        self.fill_brush(&brush);
    }

    fn fill_glyph(
        &mut self,
        glyph: GlyphId,
        brush_transform: Option<Matrix<f32>>,
        brush: Brush<'_>,
    ) {
        if !self.spend() {
            return;
        }
        self.outline(glyph);
        let outer = self.transform;
        if let Some(m) = brush_transform {
            self.transform = mul(outer, matrix(m));
        }
        self.fill_brush(&brush);
        self.transform = outer;
    }

    /// read-fonts paints a composite as a layer holding its backdrop, then
    /// a layer in its mode holding its source.
    fn push_layer(&mut self, mode: CompositeMode) {
        if !self.spend() {
            return;
        }
        self.layers.push((self.body.len(), mode));
    }

    fn pop_layer(&mut self) {
        if !self.spend() {
            return;
        }
        let Some((start, mode)) = self.layers.pop() else {
            return;
        };
        let source = self.body.split_off(start);
        // The backdrop: what the enclosing layer holds before this one.
        let from = self.layers.last().map_or(start, |&(at, _)| at);
        let backdrop = self.body.split_off(from);
        let out = if mode == CompositeMode::SrcOver {
            format!("{backdrop}<g>{source}</g>")
        } else {
            self.composite(mode, &backdrop, &source)
        };
        self.body.push_str(&out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{FontSet, FontStore};

    /// Runs `f` with the fixture fonts `EmojiTest-COLR`, `-CBDT`, `-sbix`,
    /// `limits/colr-fanout`, `limits/colr-fanout-empty` and
    /// `EmojiTest-SVG`, keyed 0 to 5.
    fn with_fonts(f: impl FnOnce(&FontSet<'_>)) {
        let mut store = FontStore::new();
        let files = [
            ("colr", "EmojiTest-COLR.ttf"),
            ("cbdt", "EmojiTest-CBDT.ttf"),
            ("sbix", "EmojiTest-sbix.ttf"),
            ("fanout", "limits/colr-fanout.ttf"),
            ("fanout-empty", "limits/colr-fanout-empty.ttf"),
            ("svg", "EmojiTest-SVG.ttf"),
        ];
        for (id, file) in files {
            let bytes = crate::test_fixtures::fixture(&format!("fonts/{file}"));
            store.register(id, bytes).unwrap();
        }
        let json = r#"{"version":1,"fonts":[{"id":"colr","family":"a"},{"id":"cbdt","family":"b"},
            {"id":"sbix","family":"c"},{"id":"fanout","family":"d"},{"id":"fanout-empty","family":"e"},
            {"id":"svg","family":"f"}],"pages":[]}"#;
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
            // Alpha 0.8, as the font gives it.
            assert!(grin.contains(r#"fill-opacity="0.8""#), "{grin}");
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
            // A sweep gradient: wedges of a degree around its centre, from
            // purple to green, merged where the colour is the same, clipped
            // to the outline; past its end angle, the last stop.
            let (woman, _) = svg(colr, '👩');
            assert!(woman.contains(r#"fill="rgb(90,30,140)""#), "{woman}");
            assert!(woman.contains(r#"fill="rgb(40,160,90)""#), "{woman}");
            let wedges = woman.matches("<path d=\"M5").count();
            assert!((100..=360).contains(&wedges), "{wedges}");
            assert!(woman.contains("<clipPath"), "{woman}");
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

    /// The glyphs of an `SVG ` table: from a compressed document of their
    /// own, its `viewBox` fitted to the em; from a document of several,
    /// without the others; nested in a group, by reference.
    #[test]
    fn svg_table_glyphs_are_drawn_alone_in_font_units() {
        with_fonts(|fonts| {
            let font = fonts.get(5);
            assert!(font.has_color());
            // Half size in a viewBox half the em: x 100..1000, y -100..800.
            let (grin, bbox) = svg(font, '😀');
            assert_eq!(bbox, [100.0, -100.0, 1000.0, 800.0], "{grin}");
            assert!(grin.contains("<radialGradient"), "{grin}");
            assert!(
                grin.contains(r#"transform="matrix(2 0 0 2 0 0)""#),
                "{grin}"
            );
            assert!(!grin.contains("<?xml"), "{grin}");
            // The heart keeps the gradient it shares, not the star.
            let (heart, bbox) = svg(font, '\u{2764}');
            assert!(heart.contains("<linearGradient"), "{heart}");
            assert!(
                !heart.contains("context-fill") && !heart.contains("<circle"),
                "{heart}"
            );
            assert_eq!(bbox[3], 640.0, "{heart}");
            // The star takes the text colour.
            let star = font.glyph('\u{2B50}').unwrap();
            assert!(font.color_glyph_tinted(star).unwrap().1);
            assert!(
                !font
                    .color_glyph_tinted(font.glyph('😀').unwrap())
                    .unwrap()
                    .1
            );
            let Some(ColorGlyph::Svg { svg: red, .. }) = font.color_glyph_in(star, [255, 0, 0])
            else {
                panic!("no colour")
            };
            assert!(red.contains(r#"color="rgb(255,0,0)""#), "{red}");
            assert!(red.contains(r#"fill="currentColor""#), "{red}");
            // Nested: drawn by reference; its box is its own.
            let (thumbs, bbox) = svg(font, '👍');
            assert!(thumbs.contains(r##"<use href="#glyph5"/>"##), "{thumbs}");
            assert_eq!(bbox, [250.0, -100.0, 650.0, 500.0]);
            let (_, bbox) = svg(font, '\u{1F3C1}');
            assert_eq!(bbox, [50.0, 0.0, 1050.0, 700.0]);
            assert_eq!(font.color_glyph(font.glyph(' ').unwrap()), None);
        });
    }

    #[test]
    fn svg_documents_inflate_within_the_limit() {
        let small = b"<svg/>";
        assert_eq!(svg_document_text(small).as_deref(), Some(&small[..]));
        let gz = |data: &[u8]| {
            use std::io::Write;
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        };
        assert_eq!(svg_document_text(&gz(small)).as_deref(), Some(&small[..]));
        let big = vec![b' '; MAX_SVG_BYTES + 1];
        assert_eq!(svg_document_text(&gz(&big)), None);
        assert_eq!(svg_document_text(&big), None);
        assert_eq!(
            svg_document_text(&gz(&big[1..])).map(|t| t.len()),
            Some(MAX_SVG_BYTES)
        );
    }

    /// Composites blend with the PDF's blend modes, and the Porter-Duff
    /// operators mask the source or the backdrop with the other.
    #[test]
    fn composites_blend_and_mask() {
        with_fonts(|fonts| {
            let colr = fonts.get(0);
            let (art, _) = svg(colr, '\u{1F3A8}');
            // Multiply, in a group of its own.
            assert!(art.contains("isolation:isolate"), "{art}");
            assert!(art.contains("mix-blend-mode:multiply"), "{art}");
            // Source-in: the source in the backdrop's alpha.
            assert!(art.contains(r#"mask-type="alpha""#), "{art}");
            // Destination-out: the backdrop where the source is not.
            assert!(art.contains(r#"fill="white""#), "{art}");
            assert_eq!(art.matches("<mask ").count(), 3, "{art}");
            // Each part is written once, and drawn by reference.
            assert_eq!(art.matches(r#"fill="rgb(220,30,60)""#).count(), 1, "{art}");
            assert!(crate::images::svg::parse(art.as_bytes()).is_ok());
        });
    }

    #[test]
    fn sweep_colours_extend_past_the_stops() {
        let stops = [(0.25, [0, 0, 0, 255]), (0.75, [255, 0, 0, 255])];
        let red = |extend, t| (sweep_color(&stops, extend, t)[0] * 255.0).round();
        assert_eq!(red(Extend::Pad, 0.5), 128.0);
        assert_eq!(red(Extend::Pad, -3.0), 0.0);
        assert_eq!(red(Extend::Pad, 3.0), 255.0);
        // Repeat starts again at the first stop; reflect turns back.
        assert_eq!(red(Extend::Repeat, 0.875), 64.0);
        assert_eq!(red(Extend::Reflect, 0.875), 191.0);
        assert_eq!(red(Extend::Reflect, 0.25 - 0.125), 64.0);
        assert_eq!(red(Extend::Pad, f32::NAN), 0.0);
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
            // Its paints call the painter, or they never do: either way it
            // stops as fast as an ordinary colour glyph is drawn.
            let colr = fonts.get(0);
            for font in [fonts.get(3), fonts.get(4)] {
                crate::test_timing::assert_as_fast(
                    font.id,
                    || {
                        let t = std::time::Instant::now();
                        assert!(colr.color_glyph(colr.glyph('😀').unwrap()).is_some());
                        t.elapsed()
                    },
                    || {
                        let t = std::time::Instant::now();
                        assert_eq!(font.color_glyph(font.glyph('😀').unwrap()), None);
                        t.elapsed()
                    },
                );
            }
        });
    }

    /// A glyph whose paint graph is within the limit can still call the
    /// painter more often than that: the calls past the budget do nothing,
    /// and the glyph has no colour.
    #[test]
    fn painter_calls_past_the_budget_leave_no_colour() {
        with_fonts(|fonts| {
            let painted = |calls: usize| {
                let mut p = SvgPainter::new(&fonts.get(0).font, BLACK_TEXT);
                p.push_clip_box(BoundingBox {
                    x_min: 0.0,
                    y_min: 0.0,
                    x_max: 10.0,
                    y_max: 10.0,
                });
                for _ in 1..calls {
                    p.push_layer(CompositeMode::SrcOver);
                }
                p.finish()
            };
            assert!(painted(MAX_PAINT_OPS).is_some());
            assert_eq!(painted(MAX_PAINT_OPS + 1), None);
        });
    }

    /// A paint table of a test `COLR` table.
    enum P {
        /// `PaintColrLayers`: `count` layers from layer `first`.
        Layers(u8, u32),
        /// `PaintColrGlyph`.
        ColrGlyph(u16),
        /// `PaintGlyph` of glyph 0, over paint table `n`.
        Glyph(usize),
        /// `PaintSolid`.
        Solid,
    }

    /// A version 1 `COLR` table: `glyphs` (glyph id, paint table index),
    /// sorted by id; `layers`, paint table indices; then the paint tables.
    fn colr(glyphs: &[(u16, usize)], layers: &[usize], paints: &[P]) -> Vec<u8> {
        let size = |p: &P| match p {
            P::Layers(..) | P::Glyph(_) => 6,
            P::ColrGlyph(_) => 3,
            P::Solid => 5,
        };
        let base_list = 34;
        let layer_list = base_list + 4 + 6 * glyphs.len();
        let first_paint = layer_list + 4 + 4 * layers.len();
        let mut at = Vec::new();
        let mut next = first_paint;
        for p in paints {
            at.push(next);
            next += size(p);
        }
        let u32b = |v: usize| u32::try_from(v).unwrap().to_be_bytes();
        let mut t = Vec::new();
        t.extend_from_slice(&[0, 1, 0, 0]);
        t.extend_from_slice(&[0; 10]);
        t.extend_from_slice(&u32b(base_list));
        t.extend_from_slice(&u32b(layer_list));
        t.extend_from_slice(&[0; 12]);
        t.extend_from_slice(&u32b(glyphs.len()));
        for &(gid, p) in glyphs {
            t.extend_from_slice(&gid.to_be_bytes());
            t.extend_from_slice(&u32b(at[p] - base_list));
        }
        t.extend_from_slice(&u32b(layers.len()));
        for &p in layers {
            t.extend_from_slice(&u32b(at[p] - layer_list));
        }
        for (k, p) in paints.iter().enumerate() {
            match *p {
                P::Layers(count, first) => {
                    t.push(1);
                    t.push(count);
                    t.extend_from_slice(&first.to_be_bytes());
                }
                P::ColrGlyph(gid) => {
                    t.push(11);
                    t.extend_from_slice(&gid.to_be_bytes());
                }
                P::Glyph(child) => {
                    t.push(10);
                    t.extend_from_slice(&u32b(at[child] - at[k])[1..]);
                    t.extend_from_slice(&[0, 0]);
                }
                P::Solid => t.extend_from_slice(&[2, 0, 0, 0x40, 0]),
            }
        }
        t
    }

    #[test]
    fn paint_graphs_count_each_table_as_often_as_it_is_reached() {
        // Three layers, each a glyph over one shared fill: 1 + 3 × 2.
        let t = colr(
            &[(1, 0)],
            &[1, 1, 1],
            &[P::Layers(3, 0), P::Glyph(2), P::Solid],
        );
        assert!(paint_graph_within(&t, 1, 7));
        assert!(!paint_graph_within(&t, 1, 6));
        // A glyph without colour data.
        assert!(paint_graph_within(&t, 9, 0));
        // A table cut short, which ttf-parser does not read either.
        assert!(!paint_graph_within(&t[..40], 1, 7));
    }

    /// Glyphs that only name other glyphs paint nothing, so they never call
    /// the painter, and are still bounded: eight levels of 255 layers would
    /// be visited 255⁸ times.
    #[test]
    fn paint_graphs_that_fan_out_are_over_without_painting() {
        // Glyph `level` paints 255 layers, each the glyph of the next level;
        // the last glyph is one fill. Nothing calls the painter but the
        // fill, which eight levels reach 255⁸ times.
        let mut glyphs = Vec::new();
        let mut layers = Vec::new();
        let mut paints = Vec::new();
        for level in 0..8u16 {
            glyphs.push((level, paints.len()));
            paints.push(P::Layers(255, u32::from(level) * 255));
            layers.extend(std::iter::repeat_n(paints.len(), 255));
            paints.push(P::ColrGlyph(level + 1));
        }
        glyphs.push((8, paints.len()));
        paints.push(P::Solid);
        let t = colr(&glyphs, &layers, &paints);
        assert!(!paint_graph_within(&t, 0, MAX_PAINT_OPS));
        // The last level: its layers, their glyph paints and the fill.
        assert!(paint_graph_within(&t, 7, 1 + 255 * 2));
        assert!(!paint_graph_within(&t, 7, 255 * 2));
        // Each table is counted once, however often it is reached.
        let mut g = PaintGraph::new(&t, usize::MAX).unwrap();
        assert_eq!(g.glyph(5), Some(1 + 255 * (2 + 255 * (2 + 255 * 2))));
        assert!(g.counted.len() <= paints.len());
    }

    #[test]
    fn cycles_and_deep_nesting_are_over() {
        // A glyph that paints itself.
        let t = colr(&[(1, 0)], &[], &[P::ColrGlyph(1)]);
        assert!(!paint_graph_within(&t, 1, MAX_PAINT_OPS));
        // A chain of glyph paints: as deep as ttf-parser follows, and one more.
        let chain = |n: usize| {
            let mut paints: Vec<P> = (1..n).map(P::Glyph).collect();
            paints.push(P::Solid);
            colr(&[(1, 0)], &[], &paints)
        };
        assert!(paint_graph_within(
            &chain(MAX_PAINT_DEPTH),
            1,
            MAX_PAINT_OPS
        ));
        assert!(!paint_graph_within(
            &chain(MAX_PAINT_DEPTH + 1),
            1,
            MAX_PAINT_OPS
        ));
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
