//! SVG → PDF: a `usvg` tree becomes a Form `XObject` with vector content.
//!
//! - Paths keep their geometry; quadratic segments become cubics.
//! - Solid paints are RGB colours; gradients are axial/radial shading
//!   patterns, with a luminosity soft mask when stop opacities vary, and a
//!   function that repeats or mirrors the stops over what is painted for
//!   `spreadMethod` `repeat` and `reflect`; patterns are coloured tiling
//!   patterns.
//! - Group opacity, blend modes, masks and clip paths that are not a
//!   single path go through transparency groups and soft masks. A clip
//!   path that is one path becomes a plain `W n` clip.
//! - Embedded JPEG/PNG/GIF/WebP images reuse the raster image writer, once
//!   per distinct image however many copies draw it; nested SVGs are drawn
//!   inline.
//! - Filters are not supported: filtered elements are drawn without them.
//! - Text: a render's copy of an SVG with text has a "probe" path where
//!   each text is (`images/svg_text`). A probe draws its text, laid out
//!   with the render's fonts, as PDF text with the probe's paint: fill and
//!   stroke in its paint order, underlines and overlines under the glyphs
//!   and line-throughs over them. Its colour glyphs and emoji images are
//!   images, each in a span whose `ActualText` is its text, as on pages.
//!
//! Patterns and shadings are positioned relative to the form they are used
//! in, so the transform from the current form's space is tracked (`ts`)
//! and reset for every new form.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use pdf_writer::types::{
    BlendMode, ColorSpaceOperand, FunctionShadingType, LineCapStyle, LineJoinStyle, MaskType,
    PaintType, TextRenderingMode, TilingType,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};
use usvg::tiny_skia_path::{self, PathSegment, Point};
use usvg::{Group, Node, Paint, Transform};

use super::{alpha_key, key_alpha, number, Out, Profile, RefAlloc};
use crate::fonts::{FontKey, FontSet};
use crate::images::svg::ProbedText;
use crate::images::svg_text::Probes;
use crate::images::{ContentKey, ImageBody, ImageKey, ImageSet};
use crate::paint::{span, InvisibleGlyph, Op, TextGlyph, TextOp, NO_RADII};
use crate::text::{ColorGlyphs, LaidText};
use crate::{flate, InkoxideError};

/// Most path segments one SVG may emit, copies included.
const MAX_SEGMENTS: usize = 5_000_000;
/// Deepest group nesting followed (usvg trees from checked input stay far
/// below this).
const MAX_DEPTH: u32 = 256;
/// Most periods of a repeated or reflected gradient written; past them it
/// pads. Each is an entry in a stitching function.
const MAX_PERIODS: f32 = 1000.0;

/// What SVG text is drawn with: the render's fonts, its colour glyphs and
/// the text they extract as, and its images.
#[derive(Clone, Copy)]
pub(crate) struct TextInk<'a, 'f> {
    pub fonts: &'a FontSet<'f>,
    pub colors: &'a ColorGlyphs,
    pub images: &'a ImageSet,
}

/// [`TextInk`], and the font and image objects written for it.
#[derive(Clone, Copy)]
pub(crate) struct TextResources<'a, 'f> {
    pub ink: TextInk<'a, 'f>,
    pub refs: &'a BTreeMap<FontKey, Ref>,
    pub image_refs: &'a BTreeMap<ImageKey, Ref>,
}

/// What the text of a render's copy of an SVG draws besides itself: the
/// glyphs its fonts must embed (those it may draw, and the spaces its
/// images extract through), and the images it draws (colour glyphs and
/// emoji).
pub(crate) fn text_uses(
    probed: &ProbedText,
    res: TextInk<'_, '_>,
) -> (Vec<(FontKey, u16)>, Vec<ImageKey>) {
    let mut glyphs = Vec::new();
    let mut keys = Vec::new();
    for t in &probed.texts {
        // A font without outlines is never embedded.
        let outlined = t
            .glyphs
            .iter()
            .filter(|g| res.fonts.get(g.font).has_outlines());
        glyphs.extend(outlined.map(|g| (g.font, g.id)));
        for (_, ops) in image_ops(t, None, res) {
            for op in ops {
                match op {
                    Op::Image { key, .. } => keys.push(key),
                    Op::InvisibleText(g) => glyphs.push((g.font, g.glyph)),
                    _ => {}
                }
            }
        }
    }
    (glyphs, keys)
}

/// Whether SVG text may draw image `key`: not an SVG with text of its own,
/// so that no form draws itself.
fn drawable(images: &ImageSet, key: ImageKey) -> bool {
    !matches!(
        images.get(key).body,
        ImageBody::Svg { text: Some(_), .. }
            | ImageBody::Svg {
                probed: Some(_),
                ..
            }
    )
}

/// The font of the invisible space in the span of an image in SVG text:
/// `preferred` (the font of an emoji image's span), or the first font of
/// the document with outlines and a space, preferring one without colour
/// glyphs, as in page text.
fn strut(fonts: &FontSet<'_>, preferred: Option<FontKey>) -> Option<FontKey> {
    let spaced = |k: FontKey| InvisibleGlyph::space(fonts, k, 1.0, (0.0, 0.0), 1.0).is_some();
    let all = || 0..fonts.fonts.len() as FontKey;
    preferred
        .filter(|&k| spaced(k))
        .or_else(|| all().find(|&k| !fonts.get(k).has_color() && spaced(k)))
        .or_else(|| all().find(|&k| spaced(k)))
}

/// The images of `text` in paint style `paint` (`None`: all), as page text
/// draws them, each with the frame of its glyph: its colour glyphs
/// ([`ColorGlyphs::push`]) and emoji images, in spans with their text.
fn image_ops(text: &LaidText, paint: Option<u32>, res: TextInk<'_, '_>) -> Vec<(Frame, Vec<Op>)> {
    let (glyphs, images) = match paint {
        Some(p) => (text.glyphs_of(p), text.images_of(p)),
        None => (&text.glyphs[..], &text.images[..]),
    };
    let mut out = Vec::new();
    let colored = glyphs.iter().filter(|g| res.colors.draws(g.font, g.id));
    for g in colored {
        let op = TextOp {
            font: g.font,
            size: g.size,
            color: [0.0, 0.0, 0.0, 1.0],
            x: 0.0,
            y: 0.0,
            glyphs: vec![TextGlyph {
                id: g.id,
                x: 0.0,
                y_offset: 0.0,
            }],
        };
        let mut ops = Vec::new();
        let strut = strut(res.fonts, None);
        res.colors.push(
            op,
            strut,
            res.fonts,
            res.images,
            &[text.glyph_text(g)],
            &mut ops,
        );
        out.push((Frame::of(g.x, g.y, g.rotate, g.scale_x), ops));
    }
    for i in images {
        let mut ops = Vec::new();
        let draw = |ops: &mut Vec<Op>| {
            if drawable(res.images, i.key) {
                res.images
                    .paint_in(i.key, i.rect, i.placement, &NO_RADII, ops);
            }
        };
        match i.text.as_deref().filter(|t| !t.is_empty()) {
            Some(text) => {
                let space = strut(res.fonts, Some(i.font)).and_then(|k| {
                    InvisibleGlyph::space(res.fonts, k, i.size, (0.0, 0.0), i.rect.w)
                });
                span(&mut ops, text, space, draw);
            }
            None => draw(&mut ops),
        }
        out.push((Frame::of(i.x, i.y, i.rotate, i.scale_x), ops));
    }
    out
}

/// Where an image in SVG text goes: the place and turn of its glyph, as a
/// matrix from the glyph's frame to the text's user space (y down).
#[derive(Clone, Copy)]
struct Frame([f32; 6]);

impl Frame {
    fn of(x: f32, y: f32, rotate: f32, scale_x: f32) -> Self {
        let (sin, cos) = if rotate == 0.0 {
            (0.0, 1.0)
        } else {
            rotate.to_radians().sin_cos()
        };
        Frame([cos * scale_x, sin * scale_x, -sin, cos, x, y])
    }
}

/// Writes `tree` as a Form `XObject` mapping the SVG viewport, of `size`,
/// onto the unit square (y up), like an image `XObject`. `text` draws the
/// text of a render's copy of an SVG at its probes.
pub(crate) fn write_svg(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    out: &mut Out,
    (tree, size): (&usvg::Tree, usvg::Size),
    profile: Profile,
    text: Option<(&ProbedText, TextResources<'_, '_>)>,
) -> Result<Ref, InkoxideError> {
    let (w, h) = (size.width(), size.height());
    let mut s = Svg {
        pdf,
        alloc,
        out,
        alphas: HashMap::new(),
        images: HashMap::new(),
        segments: 0,
        profile,
        text,
    };
    let mut cv = Canvas::new();
    s.group(&mut cv, tree.root(), Scope::form(0, false))?;
    let id = s.alloc.next();
    s.write_form(
        id,
        cv,
        Rect::new(0.0, 0.0, w, h),
        Some([1.0 / w, 0.0, 0.0, -1.0 / h, 0.0, 1.0]),
        false,
    )?;
    Ok(id)
}

/// The kinds of resource a form's content names, in the order its
/// resource dictionary lists them.
#[derive(Clone, Copy)]
enum ResKind {
    GraphicsState,
    XObject,
    Pattern,
    Shading,
    Font,
}

impl ResKind {
    const ALL: [Self; 5] = [
        Self::GraphicsState,
        Self::XObject,
        Self::Pattern,
        Self::Shading,
        Self::Font,
    ];

    /// The resource names of this kind are the prefix and a number.
    fn prefix(self) -> &'static str {
        match self {
            Self::GraphicsState => "G",
            Self::XObject => "X",
            Self::Pattern => "P",
            Self::Shading => "S",
            Self::Font => "F",
        }
    }

    /// The dictionary of `res` that holds resources of this kind.
    fn dict<'a>(self, res: &'a mut pdf_writer::writers::Resources<'_>) -> pdf_writer::Dict<'a> {
        match self {
            Self::GraphicsState => res.ext_g_states(),
            Self::XObject => res.x_objects(),
            Self::Pattern => res.patterns(),
            Self::Shading => res.shadings(),
            Self::Font => res.fonts(),
        }
    }
}

/// A content stream under construction and the resources it names.
struct Canvas {
    c: Content,
    /// Resources by kind (see [`ResKind::ALL`]); a resource's position is
    /// the number in its name.
    res: [Vec<Ref>; 5],
}

impl Canvas {
    fn new() -> Self {
        Self {
            c: Content::new(),
            res: Default::default(),
        }
    }

    /// The name `r` goes by in this content, added to its resources the
    /// first time.
    fn name(&mut self, kind: ResKind, r: Ref) -> String {
        let list = &mut self.res[kind as usize];
        let i = list.iter().position(|&x| x == r).unwrap_or_else(|| {
            list.push(r);
            list.len() - 1
        });
        format!("{}{i}", kind.prefix())
    }

    fn set_gs(&mut self, r: Ref) {
        let name = self.name(ResKind::GraphicsState, r);
        self.c.set_parameters(Name(name.as_bytes()));
    }

    fn draw_xobject(&mut self, r: Ref) {
        let name = self.name(ResKind::XObject, r);
        self.c.x_object(Name(name.as_bytes()));
    }

    fn set_pattern(&mut self, r: Ref, stroke: bool) {
        let name = self.name(ResKind::Pattern, r);
        if stroke {
            self.c.set_stroke_color_space(ColorSpaceOperand::Pattern);
            self.c.set_stroke_pattern(None, Name(name.as_bytes()));
        } else {
            self.c.set_fill_color_space(ColorSpaceOperand::Pattern);
            self.c.set_fill_pattern(None, Name(name.as_bytes()));
        }
    }

    fn paint_shading(&mut self, r: Ref) {
        let name = self.name(ResKind::Shading, r);
        self.c.shading(Name(name.as_bytes()));
    }

    fn write_resources(&self, mut res: pdf_writer::writers::Resources<'_>) {
        for kind in ResKind::ALL {
            let list = &self.res[kind as usize];
            if list.is_empty() {
                continue;
            }
            let mut d = kind.dict(&mut res);
            for (i, r) in list.iter().enumerate() {
                d.pair(Name(format!("{}{i}", kind.prefix()).as_bytes()), *r);
            }
        }
    }
}

/// A transform as a PDF matrix. Transforms compound (a tiny `viewBox`
/// scaled up, nested scales) and take any number (`scale(1e30)`), so they
/// can pass what readers parse, or overflow `f32`; see [`number`].
fn matrix(t: Transform) -> [f32; 6] {
    [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(number)
}

/// `r` with every corner through [`number`]: SVG sizes, positions and
/// bounding boxes take any number.
fn bounded(r: Rect) -> Rect {
    Rect::new(number(r.x1), number(r.y1), number(r.x2), number(r.y2))
}

/// A gradient's shading geometry: axial (`[x1 y1 x2 y2]`) or radial
/// (`[fx fy fr cx cy r]`) coordinates at parameters 0 and 1.
type Geometry = (FunctionShadingType, Vec<f32>);

/// The parameter range a repeated or reflected gradient covers (whole
/// periods), and whether it mirrors every other period.
#[derive(Clone, Copy)]
struct Periods {
    from: f32,
    to: f32,
    reflect: bool,
}

impl Periods {
    /// The periods of `geometry` that cover `bbox` (the painted area, in
    /// the space `gt` maps the gradient into); `None` pads (`spreadMethod`
    /// `pad`, or a range that cannot be bounded).
    fn cover(
        spread: usvg::SpreadMethod,
        (kind, c): &Geometry,
        gt: Transform,
        bbox: Rect,
    ) -> Option<Self> {
        let reflect = match spread {
            usvg::SpreadMethod::Pad => return None,
            usvg::SpreadMethod::Reflect => true,
            usvg::SpreadMethod::Repeat => false,
        };
        let inv = gt.invert()?;
        let mut corners = [
            Point::from_xy(bbox.x1, bbox.y1),
            Point::from_xy(bbox.x2, bbox.y1),
            Point::from_xy(bbox.x1, bbox.y2),
            Point::from_xy(bbox.x2, bbox.y2),
        ];
        inv.map_points(&mut corners);
        let (lo, hi) = if *kind == FunctionShadingType::Axial {
            let (dx, dy) = (c[2] - c[0], c[3] - c[1]);
            let len2 = dx * dx + dy * dy;
            if len2 <= 0.0 {
                return None;
            }
            let t = |p: &Point| ((p.x - c[0]) * dx + (p.y - c[1]) * dy) / len2;
            let ts = corners.iter().map(t);
            (
                ts.clone().fold(f32::MAX, f32::min),
                ts.fold(f32::MIN, f32::max),
            )
        } else {
            // The circle at t has centre f + t·(c − f) and radius
            // fr + t·(r − fr): it holds a point at distance d from the
            // focus once fr + t·(r − fr) ≥ d + t·|c − f|.
            let (fx, fy, fr, cx, cy, r) = (c[0], c[1], c[2], c[3], c[4], c[5]);
            let grow = r - fr - (cx - fx).hypot(cy - fy);
            if grow <= 0.0 {
                return None;
            }
            let t = |p: &Point| ((p.x - fx).hypot(p.y - fy) - fr) / grow;
            (0.0, corners.iter().map(t).fold(1.0, f32::max))
        };
        let from = lo.min(0.0).floor().max(-MAX_PERIODS);
        let to = hi.max(1.0).ceil().min(from + MAX_PERIODS);
        (from.is_finite() && to.is_finite()).then_some(Self { from, to, reflect })
    }

    /// `geometry`'s coordinates at `from` and `to`.
    fn coords(&self, (kind, c): &Geometry) -> Vec<f32> {
        let at = |t: f32, a: f32, b: f32| a + t * (b - a);
        if *kind == FunctionShadingType::Axial {
            let p = |t| [at(t, c[0], c[2]), at(t, c[1], c[3])];
            [p(self.from), p(self.to)].concat()
        } else {
            let p = |t| [at(t, c[0], c[3]), at(t, c[1], c[4]), at(t, c[2], c[5])];
            [p(self.from), p(self.to)].concat()
        }
    }
}

fn pdf_rect(r: usvg::NonZeroRect) -> Rect {
    Rect::new(r.left(), r.top(), r.right(), r.bottom())
}

fn blend_mode(m: usvg::BlendMode) -> BlendMode {
    use usvg::BlendMode as B;
    match m {
        B::Normal => BlendMode::Normal,
        B::Multiply => BlendMode::Multiply,
        B::Screen => BlendMode::Screen,
        B::Overlay => BlendMode::Overlay,
        B::Darken => BlendMode::Darken,
        B::Lighten => BlendMode::Lighten,
        B::ColorDodge => BlendMode::ColorDodge,
        B::ColorBurn => BlendMode::ColorBurn,
        B::HardLight => BlendMode::HardLight,
        B::SoftLight => BlendMode::SoftLight,
        B::Difference => BlendMode::Difference,
        B::Exclusion => BlendMode::Exclusion,
        B::Hue => BlendMode::Hue,
        B::Saturation => BlendMode::Saturation,
        B::Color => BlendMode::Color,
        B::Luminosity => BlendMode::Luminosity,
    }
}

/// A clip path that is exactly one filled path, not a text's probe: its
/// outline in the clip's parent space and its rule.
fn simple_clip(
    clip: &usvg::ClipPath,
    is_probe: impl Fn(&usvg::Path) -> bool,
) -> Option<(tiny_skia_path::Path, usvg::FillRule)> {
    let root = clip.root();
    if clip.clip_path().is_some() || root.clip_path().is_some() || root.mask().is_some() {
        return None;
    }
    let [Node::Path(p)] = root.children() else {
        return None;
    };
    if is_probe(p) {
        return None;
    }
    let rule = p.fill()?.rule();
    if !p.is_visible() {
        return None;
    }
    let ts = clip.transform().pre_concat(root.transform());
    let path = p.data().clone().transform(ts)?;
    Some((path, rule))
}

/// How the writer draws a clip path: as a plain clip, not at all (it
/// holds nothing, so neither does what it clips), or as soft masks.
enum Clip {
    Simple(tiny_skia_path::Path, usvg::FillRule),
    Empty,
    Masks,
}

/// How [`Svg::group`] draws clip path `cp`; [`scan_group`] follows it.
fn clip_kind(cp: &usvg::ClipPath, is_probe: impl Fn(&usvg::Path) -> bool) -> Clip {
    match simple_clip(cp, is_probe) {
        Some((path, rule)) => Clip::Simple(path, rule),
        None if cp.root().children().is_empty() && cp.clip_path().is_none() => Clip::Empty,
        None => Clip::Masks,
    }
}

/// What group `g`'s own settings draw with transparency (a transparency
/// group, a soft mask, a constant alpha or a blend mode), if anything.
/// Inside a clip path (`clip`) none of them apply. [`Svg::group`] draws
/// `g` in layers exactly when this or its clip path asks for it.
fn group_transparency(g: &Group, clip: bool) -> Option<&'static str> {
    if clip {
        None
    } else if g.mask().is_some() {
        Some("a mask")
    } else if g.opacity().get() < 1.0 {
        Some("group opacity")
    } else if g.blend_mode() != usvg::BlendMode::Normal {
        Some("a blend mode")
    } else if g.isolate() {
        Some("an isolated group")
    } else {
        None
    }
}

/// Whether a gradient's stops vary in opacity, which [`Svg::set_paint`]
/// draws with a soft mask.
fn translucent_stops(stops: &[usvg::Stop]) -> bool {
    stops.iter().any(|s| s.opacity().get() < 1.0)
}

/// The text and paint style a probe in `probes` stands for: a path of
/// exactly the four sides of a rectangle whose corners they list.
fn probe_of(p: &usvg::Path, probes: &Probes) -> Option<(usize, u32)> {
    let mut segs = p.data().segments();
    let (
        Some(PathSegment::MoveTo(a)),
        Some(PathSegment::LineTo(b)),
        Some(PathSegment::LineTo(c)),
        Some(PathSegment::LineTo(d)),
        Some(PathSegment::Close),
        None,
    ) = (
        segs.next(),
        segs.next(),
        segs.next(),
        segs.next(),
        segs.next(),
        segs.next(),
    )
    else {
        return None;
    };
    if (b.x, b.y, d.x, d.y) != (c.x, a.y, a.x, c.y) {
        return None;
    }
    let &(text, paint) = probes.get(&[a.x, a.y, c.x, c.y].map(f32::to_bits))?;
    Some((text as usize, paint))
}

/// The first thing in `tree` that [`write_svg`] would draw with
/// transparency (a transparency group, a soft mask, a constant alpha or a
/// blend mode), which PDF/A-1 forbids; `None` when it draws none. Follows
/// the writer: what [`Svg::group`] and the functions it calls draw, with
/// the same depth limit. A path in `probes` is text, which is drawn with
/// its paint like a path, but is not a simple clip.
pub(crate) fn transparency(tree: &usvg::Tree, probes: Option<&Probes>) -> Option<&'static str> {
    let empty = Probes::new();
    scan_group(tree.root(), 0, false, probes.unwrap_or(&empty))
}

/// [`Svg::group`]'s transparency.
fn scan_group(g: &Group, depth: u32, clip: bool, probes: &Probes) -> Option<&'static str> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Some(cp) = g.clip_path() {
        match clip_kind(cp, |p| probe_of(p, probes).is_some()) {
            Clip::Simple(..) => {}
            Clip::Empty => return None,
            Clip::Masks => return Some("a clip path other than a single path"),
        }
    }
    if let Some(what) = group_transparency(g, clip) {
        return Some(what);
    }
    g.children().iter().find_map(|n| match n {
        Node::Group(child) => scan_group(child, depth + 1, clip, probes),
        Node::Path(_) if clip => None,
        Node::Path(p) if p.is_visible() => {
            let fill = p.fill().map(|f| (f.paint(), f.opacity().get()));
            let stroke = p.stroke().map(|s| (s.paint(), s.opacity().get()));
            [fill, stroke]
                .into_iter()
                .flatten()
                .find_map(|(paint, opacity)| scan_paint(paint, opacity, depth, probes))
        }
        Node::Image(img) if !clip && img.is_visible() => match img.kind() {
            usvg::ImageKind::JPEG(data)
            | usvg::ImageKind::PNG(data)
            | usvg::ImageKind::GIF(data)
            | usvg::ImageKind::WEBP(data) => crate::images::decode_embedded(data)
                .ok()
                .and_then(|d| super::image::transparency(&d)),
            usvg::ImageKind::SVG(tree) => scan_group(tree.root(), depth + 1, clip, probes),
        },
        Node::Path(_) | Node::Image(_) => None,
        Node::Text(t) => scan_group(t.flattened(), depth + 1, clip, probes),
    })
}

/// [`Svg::set_paint`]'s transparency.
fn scan_paint(paint: &Paint, opacity: f32, depth: u32, probes: &Probes) -> Option<&'static str> {
    let stops = match paint {
        Paint::Color(_) => &[][..],
        Paint::LinearGradient(l) => l.stops(),
        Paint::RadialGradient(r) => r.stops(),
        Paint::Pattern(pt) => {
            if let Some(what) = scan_group(pt.root(), depth + 1, false, probes) {
                return Some(what);
            }
            &[][..]
        }
    };
    if translucent_stops(stops) {
        Some("a gradient with stop opacity")
    } else if opacity < 1.0 {
        Some("fill or stroke opacity")
    } else {
        None
    }
}

/// Where a node is drawn: the transform from the current form's space
/// (`ts`), how deep in groups, and whether as part of a clip path, where
/// everything is filled opaque black (the content of an alpha soft mask).
#[derive(Clone, Copy)]
struct Scope {
    ts: Transform,
    depth: u32,
    clip: bool,
}

impl Scope {
    /// The top of a new form, `depth` groups deep.
    fn form(depth: u32, clip: bool) -> Self {
        Self {
            ts: Transform::identity(),
            depth,
            clip,
        }
    }

    fn deeper(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

struct Svg<'a> {
    pdf: &'a mut Pdf,
    alloc: &'a mut RefAlloc,
    out: &'a mut Out,
    /// Shared graphics states for (fill alpha, stroke alpha, blend mode).
    alphas: HashMap<(u16, u16, u8), Ref>,
    /// Embedded raster images by content: the image `XObject`, `None` if
    /// it did not decode.
    images: HashMap<ContentKey<Arc<Vec<u8>>>, Option<Ref>>,
    segments: usize,
    /// Passed on to embedded raster images; under PDF/A, `.notdef` is
    /// left out of text.
    profile: Profile,
    /// The text at the probes, and the fonts to draw it with.
    text: Option<(&'a ProbedText, TextResources<'a, 'a>)>,
}

type Res = Result<(), InkoxideError>;

fn too_complex() -> InkoxideError {
    InkoxideError::Image(format!(
        "SVG is too complex: more than {MAX_SEGMENTS} path segments and glyphs"
    ))
}

impl Svg<'_> {
    fn next(&mut self) -> Ref {
        self.alloc.next()
    }

    fn write_form(
        &mut self,
        id: Ref,
        mut cv: Canvas,
        bbox: Rect,
        m: Option<[f32; 6]>,
        group: bool,
    ) -> Res {
        let data = super::deflate(
            &super::content_bytes(std::mem::replace(&mut cv.c, Content::new()), self.profile)?,
            flate::LEVEL,
        )?;
        self.out.form_xobject(self.pdf, id, &data, |f| {
            f.filter(Filter::FlateDecode);
            f.bbox(bounded(bbox));
            if let Some(m) = m {
                f.matrix(m.map(number));
            }
            if group {
                f.group().transparency().color_space().device_rgb();
            }
            cv.write_resources(f.resources());
        });
        Ok(())
    }

    /// A transparency group form over `bbox` (current space) with content
    /// drawn by `draw`, whose `ts` starts at the identity.
    fn sub_form(
        &mut self,
        bbox: Rect,
        draw: impl FnOnce(&mut Self, &mut Canvas) -> Res,
    ) -> Result<Ref, InkoxideError> {
        self.profile
            .allow_transparency("an SVG mask, clip path, opacity or blend mode")?;
        let mut cv = Canvas::new();
        draw(self, &mut cv)?;
        let id = self.next();
        self.write_form(id, cv, bbox, None, true)?;
        Ok(id)
    }

    /// Graphics state with constant alphas and a blend mode (shared).
    fn alpha_gs(
        &mut self,
        fill: f32,
        stroke: f32,
        blend: usvg::BlendMode,
    ) -> Result<Ref, InkoxideError> {
        self.profile.allow_transparency("SVG opacity")?;
        let key = (alpha_key(fill), alpha_key(stroke), blend as u8);
        if let Some(&r) = self.alphas.get(&key) {
            return Ok(r);
        }
        let r = self.next();
        let mut gs = self.pdf.ext_graphics(r);
        gs.non_stroking_alpha(key_alpha(key.0))
            .stroking_alpha(key_alpha(key.1));
        if blend != usvg::BlendMode::Normal {
            gs.blend_mode(blend_mode(blend));
        }
        gs.finish();
        self.alphas.insert(key, r);
        Ok(r)
    }

    fn soft_mask_gs(&mut self, group: Ref, kind: MaskType) -> Ref {
        let r = self.next();
        self.pdf
            .ext_graphics(r)
            .soft_mask()
            .subtype(kind)
            .group(group);
        r
    }

    fn emit_path(&mut self, c: &mut Content, path: &tiny_skia_path::Path) -> Res {
        self.segments += path.len();
        if self.segments > MAX_SEGMENTS {
            return Err(too_complex());
        }
        // Shapes that are not path data (`rect`, `line`, `image`) take any
        // number, and a simple clip path is written transformed.
        let n = number;
        let mut last = Point::zero();
        let mut start = Point::zero();
        for seg in path.segments() {
            match seg {
                PathSegment::MoveTo(p) => {
                    c.move_to(n(p.x), n(p.y));
                    (last, start) = (p, p);
                }
                PathSegment::LineTo(p) => {
                    c.line_to(n(p.x), n(p.y));
                    last = p;
                }
                PathSegment::QuadTo(p1, p) => {
                    let c1 = (
                        last.x + 2.0 / 3.0 * (p1.x - last.x),
                        last.y + 2.0 / 3.0 * (p1.y - last.y),
                    );
                    let c2 = (
                        p.x + 2.0 / 3.0 * (p1.x - p.x),
                        p.y + 2.0 / 3.0 * (p1.y - p.y),
                    );
                    c.cubic_to(n(c1.0), n(c1.1), n(c2.0), n(c2.1), n(p.x), n(p.y));
                    last = p;
                }
                PathSegment::CubicTo(p1, p2, p) => {
                    c.cubic_to(n(p1.x), n(p1.y), n(p2.x), n(p2.y), n(p.x), n(p.y));
                    last = p;
                }
                PathSegment::Close => {
                    c.close_path();
                    last = start;
                }
            }
        }
        Ok(())
    }

    /// Draws a group.
    fn group(&mut self, cv: &mut Canvas, g: &Group, at: Scope) -> Res {
        let Scope { ts, depth, clip } = at;
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let at = Scope {
            ts: ts.pre_concat(g.transform()),
            ..at
        };
        cv.c.save_state();
        if !g.transform().is_identity() {
            cv.c.transform(matrix(g.transform()));
        }
        let bbox = pdf_rect(g.layer_bounding_box());
        // Soft masks, outermost first.
        let mut masks: Vec<Ref> = Vec::new();
        if let Some(cp) = g.clip_path() {
            match clip_kind(cp, |p| self.probe(p).is_some()) {
                Clip::Simple(path, rule) => {
                    self.emit_path(&mut cv.c, &path)?;
                    match rule {
                        usvg::FillRule::NonZero => cv.c.clip_nonzero(),
                        usvg::FillRule::EvenOdd => cv.c.clip_even_odd(),
                    };
                    cv.c.end_path();
                }
                Clip::Empty => {
                    cv.c.restore_state();
                    return Ok(());
                }
                Clip::Masks => self.clip_masks(cp, bbox, depth, &mut masks)?,
            }
        }
        let (opacity, blend) = if clip {
            (1.0, usvg::BlendMode::Normal)
        } else {
            if let Some(m) = g.mask() {
                self.mask_layers(m, depth, &mut masks)?;
            }
            (g.opacity().get(), g.blend_mode())
        };
        let last = if opacity < 1.0 || blend != usvg::BlendMode::Normal {
            Some(self.alpha_gs(opacity, opacity, blend)?)
        } else {
            None
        };
        if masks.is_empty() && group_transparency(g, clip).is_none() {
            self.children(cv, g, at)?;
        } else {
            self.layered(cv, &masks, last, bbox, g, at)?;
        }
        cv.c.restore_state();
        Ok(())
    }

    /// Applies `masks[0]`, then draws the rest of the layers as a
    /// transparency group under it; the innermost group holds `g`'s
    /// children and is painted with `last` (opacity, blend mode).
    fn layered(
        &mut self,
        cv: &mut Canvas,
        masks: &[Ref],
        last: Option<Ref>,
        bbox: Rect,
        g: &Group,
        at: Scope,
    ) -> Res {
        let form = if let Some((&m, rest)) = masks.split_first() {
            cv.set_gs(m);
            self.sub_form(bbox, |s, inner| s.layered(inner, rest, last, bbox, g, at))?
        } else {
            if let Some(l) = last {
                cv.set_gs(l);
            }
            self.sub_form(bbox, |s, inner| {
                s.children(inner, g, Scope::form(at.depth, at.clip))
            })?
        };
        cv.draw_xobject(form);
        Ok(())
    }

    /// Alpha soft masks for a clip path (and the clip paths it is clipped
    /// by), appended to `masks`.
    fn clip_masks(
        &mut self,
        cp: &usvg::ClipPath,
        bbox: Rect,
        depth: u32,
        masks: &mut Vec<Ref>,
    ) -> Res {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let form = self.sub_form(bbox, |s, inner| {
            inner.c.set_fill_gray(0.0);
            inner.c.save_state();
            inner.c.transform(matrix(cp.transform()));
            let at = Scope {
                ts: cp.transform(),
                depth: depth + 1,
                clip: true,
            };
            s.group(inner, cp.root(), at)?;
            inner.c.restore_state();
            Ok(())
        })?;
        let gs = self.soft_mask_gs(form, MaskType::Alpha);
        masks.push(gs);
        if let Some(sub) = cp.clip_path() {
            self.clip_masks(sub, bbox, depth + 1, masks)?;
        }
        Ok(())
    }

    /// Soft masks for a mask (and the masks it is masked by).
    fn mask_layers(&mut self, m: &usvg::Mask, depth: u32, masks: &mut Vec<Ref>) -> Res {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let r = m.rect();
        let form = self.sub_form(pdf_rect(r), |s, inner| {
            inner.c.rect(
                number(r.x()),
                number(r.y()),
                number(r.width()),
                number(r.height()),
            );
            inner.c.clip_nonzero();
            inner.c.end_path();
            s.group(inner, m.root(), Scope::form(depth + 1, false))
        })?;
        let kind = match m.kind() {
            usvg::MaskType::Luminance => MaskType::Luminosity,
            usvg::MaskType::Alpha => MaskType::Alpha,
        };
        let gs = self.soft_mask_gs(form, kind);
        masks.push(gs);
        if let Some(sub) = m.mask() {
            self.mask_layers(sub, depth + 1, masks)?;
        }
        Ok(())
    }

    fn children(&mut self, cv: &mut Canvas, g: &Group, at: Scope) -> Res {
        for n in g.children() {
            match n {
                Node::Group(child) => self.group(cv, child, at.deeper())?,
                Node::Path(p) if self.probe(p).is_some() => self.text(cv, p, at)?,
                Node::Path(p) if at.clip => {
                    if let (true, Some(fill)) = (p.is_visible(), p.fill()) {
                        self.emit_path(&mut cv.c, p.data())?;
                        match fill.rule() {
                            usvg::FillRule::NonZero => cv.c.fill_nonzero(),
                            usvg::FillRule::EvenOdd => cv.c.fill_even_odd(),
                        };
                    }
                }
                Node::Path(p) => self.path(cv, p, at)?,
                Node::Image(img) if !at.clip => self.image(cv, img, at)?,
                Node::Image(_) => {}
                Node::Text(t) => self.group(cv, t.flattened(), at.deeper())?,
            }
        }
        Ok(())
    }

    fn path(&mut self, cv: &mut Canvas, p: &usvg::Path, at: Scope) -> Res {
        if !p.is_visible() {
            return Ok(());
        }
        let area = p.stroke_bounding_box();
        let bbox = Rect::new(area.left(), area.top(), area.right(), area.bottom());
        let fill = |s: &mut Self, cv: &mut Canvas| -> Res {
            let Some(f) = p.fill() else { return Ok(()) };
            cv.c.save_state();
            s.set_paint(cv, f.paint(), f.opacity().get(), false, bbox, at)?;
            s.emit_path(&mut cv.c, p.data())?;
            match f.rule() {
                usvg::FillRule::NonZero => cv.c.fill_nonzero(),
                usvg::FillRule::EvenOdd => cv.c.fill_even_odd(),
            };
            cv.c.restore_state();
            Ok(())
        };
        let stroke = |s: &mut Self, cv: &mut Canvas| -> Res {
            let Some(st) = p.stroke() else { return Ok(()) };
            cv.c.save_state();
            s.set_stroke(cv, st, bbox, at)?;
            s.emit_path(&mut cv.c, p.data())?;
            cv.c.stroke();
            cv.c.restore_state();
            Ok(())
        };
        match p.paint_order() {
            usvg::PaintOrder::FillAndStroke => {
                fill(self, cv)?;
                stroke(self, cv)
            }
            usvg::PaintOrder::StrokeAndFill => {
                stroke(self, cv)?;
                fill(self, cv)
            }
        }
    }

    /// Sets the stroke paint and the line style of `st`.
    fn set_stroke(&mut self, cv: &mut Canvas, st: &usvg::Stroke, bbox: Rect, at: Scope) -> Res {
        self.set_paint(cv, st.paint(), st.opacity().get(), true, bbox, at)?;
        cv.c.set_line_width(number(st.width().get()));
        cv.c.set_line_cap(match st.linecap() {
            usvg::LineCap::Butt => LineCapStyle::ButtCap,
            usvg::LineCap::Round => LineCapStyle::RoundCap,
            usvg::LineCap::Square => LineCapStyle::ProjectingSquareCap,
        });
        cv.c.set_line_join(match st.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoinStyle::MiterJoin,
            usvg::LineJoin::Round => LineJoinStyle::RoundJoin,
            usvg::LineJoin::Bevel => LineJoinStyle::BevelJoin,
        });
        cv.c.set_miter_limit(number(st.miterlimit().get()));
        if let Some(d) = st.dasharray() {
            cv.c.set_dash_pattern(d.iter().map(|&v| number(v)), number(st.dashoffset()));
        }
        Ok(())
    }

    /// The text and paint style a probe stands for (see [`probe_of`]).
    fn probe(&self, p: &usvg::Path) -> Option<(usize, u32)> {
        probe_of(p, &self.text?.0.probes)
    }

    /// Draws the text a probe stands for with the probe's paint. In a clip
    /// path the glyphs are filled, as shapes are there.
    fn text(&mut self, cv: &mut Canvas, p: &usvg::Path, at: Scope) -> Res {
        let (Some((text, paint)), Some((probed, fonts))) = (self.probe(p), self.text) else {
            return Ok(());
        };
        if !p.is_visible() {
            return Ok(());
        }
        let laid = &probed.texts[text];
        if at.clip {
            // Shapes only: colour glyphs by their outlines, no images.
            if p.fill().is_some() {
                self.draw_text(cv, laid, paint, fonts, (false, true))?;
            }
            return Ok(());
        }
        let area = p.stroke_bounding_box();
        let bbox = Rect::new(area.left(), area.top(), area.right(), area.bottom());
        let fill = |s: &mut Self, cv: &mut Canvas| -> Res {
            let Some(f) = p.fill() else { return Ok(()) };
            cv.c.save_state();
            s.set_paint(cv, f.paint(), f.opacity().get(), false, bbox, at)?;
            s.draw_text(cv, laid, paint, fonts, (false, false))?;
            cv.c.restore_state();
            Ok(())
        };
        let stroke = |s: &mut Self, cv: &mut Canvas| -> Res {
            let Some(st) = p.stroke() else { return Ok(()) };
            cv.c.save_state();
            s.set_stroke(cv, st, bbox, at)?;
            s.draw_text(cv, laid, paint, fonts, (true, false))?;
            cv.c.restore_state();
            Ok(())
        };
        match p.paint_order() {
            usvg::PaintOrder::FillAndStroke => {
                fill(self, cv)?;
                stroke(self, cv)?;
            }
            usvg::PaintOrder::StrokeAndFill => {
                stroke(self, cv)?;
                fill(self, cv)?;
            }
        }
        // Images have their own colours.
        self.text_images(cv, laid, paint, fonts)
    }

    /// The colour glyphs and emoji images of `laid` in paint style
    /// `paint`, as page text draws them: each image in a span with its
    /// text and an invisible space.
    fn text_images(
        &mut self,
        cv: &mut Canvas,
        laid: &LaidText,
        paint: u32,
        res: TextResources<'_, '_>,
    ) -> Res {
        for (frame, ops) in image_ops(laid, Some(paint), res.ink) {
            self.segments += ops.len();
            if self.segments > MAX_SEGMENTS {
                return Err(too_complex());
            }
            cv.c.save_state();
            cv.c.transform(frame.0.map(number));
            for op in ops {
                match op {
                    Op::Save => {
                        cv.c.save_state();
                    }
                    Op::Restore => {
                        cv.c.restore_state();
                    }
                    Op::ClipRect(b) => {
                        cv.c.rect(number(b.x), number(b.y), number(b.w), number(b.h));
                        cv.c.clip_nonzero();
                        cv.c.end_path();
                    }
                    Op::Image { key, matrix } => {
                        let Some(&r) = res.image_refs.get(&key) else {
                            continue;
                        };
                        cv.c.transform(matrix.map(number));
                        let name = cv.name(ResKind::XObject, r);
                        cv.c.x_object(Name(name.as_bytes()));
                    }
                    Op::Span { actual_text } => {
                        let text = super::out::text_bytes(&actual_text);
                        cv.c.begin_marked_content_with_properties(Name(b"Span"))
                            .properties()
                            .pair(Name(b"ActualText"), Str(&text));
                    }
                    Op::EndSpan => {
                        cv.c.end_marked_content();
                    }
                    Op::InvisibleText(g) => {
                        if let Some(&r) = res.refs.get(&g.font) {
                            let name = cv.name(ResKind::Font, r);
                            super::invisible_glyph(
                                &mut cv.c,
                                &g,
                                name.as_bytes(),
                                res.ink.fonts,
                                self.profile,
                            );
                        }
                    }
                    // Colour glyphs and images draw nothing else.
                    _ => {}
                }
            }
            cv.c.restore_state();
        }
        Ok(())
    }

    /// In a clip path (`clip`), colour glyphs are drawn by their outlines.
    fn draw_text(
        &mut self,
        cv: &mut Canvas,
        laid: &LaidText,
        paint: u32,
        fonts: TextResources<'_, '_>,
        (stroke, clip): (bool, bool),
    ) -> Res {
        self.decorations(cv, laid, paint, false, stroke)?;
        self.glyphs(cv, laid, paint, fonts, (stroke, clip))?;
        self.decorations(cv, laid, paint, true, stroke)
    }

    fn decorations(
        &mut self,
        cv: &mut Canvas,
        laid: &LaidText,
        paint: u32,
        over: bool,
        stroke: bool,
    ) -> Res {
        let mut any = false;
        for d in laid.decorations_of(paint).iter().filter(|d| d.over == over) {
            self.segments += 5;
            if self.segments > MAX_SEGMENTS {
                return Err(too_complex());
            }
            let [a, b, c, e] = d.quad.map(|p| p.map(number));
            cv.c.move_to(a[0], a[1]);
            cv.c.line_to(b[0], b[1]);
            cv.c.line_to(c[0], c[1]);
            cv.c.line_to(e[0], e[1]);
            cv.c.close_path();
            any = true;
        }
        if any {
            if stroke {
                cv.c.stroke();
            } else {
                cv.c.fill_nonzero();
            }
        }
        Ok(())
    }

    /// The glyphs of `laid` in paint style `paint`: glyphs on one
    /// baseline with the same font and size share a `TJ`, with
    /// adjustments where their positions differ from the font's advances
    /// (as `show_text` writes page text); a turned glyph has its own.
    /// Colour glyphs with an image are drawn as images, but in a clip path.
    fn glyphs(
        &mut self,
        cv: &mut Canvas,
        laid: &LaidText,
        paint: u32,
        fonts: TextResources<'_, '_>,
        (stroke, clip): (bool, bool),
    ) -> Res {
        let glyphs: Vec<_> = laid
            .glyphs_of(paint)
            .iter()
            .filter(|g| !(self.profile.is_pdf_a() && g.id == 0))
            .filter(|g| clip || !fonts.ink.colors.draws(g.font, g.id))
            .filter(|g| fonts.refs.contains_key(&g.font))
            .collect();
        if glyphs.is_empty() {
            return Ok(());
        }
        self.segments += glyphs.len();
        if self.segments > MAX_SEGMENTS {
            return Err(too_complex());
        }
        cv.c.begin_text();
        if stroke {
            cv.c.set_text_rendering_mode(TextRenderingMode::Stroke);
        }
        let mut font = None;
        let mut i = 0;
        while i < glyphs.len() {
            let g0 = glyphs[i];
            if font != Some(g0.font) {
                let name = cv.name(ResKind::Font, fonts.refs[&g0.font]);
                cv.c.set_font(Name(name.as_bytes()), 1.0);
                font = Some(g0.font);
            }
            let (sin, cos) = if g0.rotate == 0.0 {
                (0.0, 1.0)
            } else {
                g0.rotate.to_radians().sin_cos()
            };
            let (size, k) = (g0.size, g0.scale_x);
            // Text space is y up; SVG user space is y down.
            cv.c.set_text_matrix(
                [
                    cos * size * k,
                    sin * size * k,
                    sin * size,
                    -cos * size,
                    g0.x,
                    g0.y,
                ]
                .map(number),
            );
            let loaded = fonts.ink.fonts.get(g0.font);
            let em = size * k;
            let mut shown = cv.c.show_positioned();
            let mut items = shown.items();
            let mut buf: Vec<u8> = Vec::new();
            let mut pen = g0.x;
            let start = i;
            while i < glyphs.len() {
                let g = glyphs[i];
                let same = g.font == g0.font
                    && g.size == size
                    && g.scale_x == k
                    && g.y == g0.y
                    && g.rotate == 0.0
                    && g0.rotate == 0.0;
                if i > start && !same {
                    break;
                }
                let delta = g.x - pen;
                if delta.abs() > super::KERN_EPS && em != 0.0 {
                    if !buf.is_empty() {
                        items.show(Str(&buf));
                        buf.clear();
                    }
                    items.adjust(number(-delta * 1000.0 / em));
                }
                buf.extend_from_slice(&g.id.to_be_bytes());
                pen = g.x + loaded.advance(g.id) * loaded.scale(em);
                i += 1;
            }
            if !buf.is_empty() {
                items.show(Str(&buf));
            }
        }
        cv.c.end_text();
        Ok(())
    }

    /// Sets the fill or stroke paint and its opacity. `bbox` bounds what
    /// will be painted (for gradient opacity masks).
    fn set_paint(
        &mut self,
        cv: &mut Canvas,
        paint: &Paint,
        opacity: f32,
        stroke: bool,
        bbox: Rect,
        at: Scope,
    ) -> Res {
        let (f, s) = if stroke {
            (1.0, opacity)
        } else {
            (opacity, 1.0)
        };
        let mut alpha_mask = None;
        match paint {
            Paint::Color(c) => {
                let rgb = [c.red, c.green, c.blue].map(|v| f32::from(v) / 255.0);
                if stroke {
                    cv.c.set_stroke_rgb(rgb[0], rgb[1], rgb[2]);
                } else {
                    cv.c.set_fill_rgb(rgb[0], rgb[1], rgb[2]);
                }
            }
            Paint::LinearGradient(_) | Paint::RadialGradient(_) => {
                let (stops, gt, spread, kind) = match paint {
                    Paint::LinearGradient(l) => (
                        l.stops(),
                        l.transform(),
                        l.spread_method(),
                        (
                            FunctionShadingType::Axial,
                            vec![l.x1(), l.y1(), l.x2(), l.y2()],
                        ),
                    ),
                    Paint::RadialGradient(r) => (
                        r.stops(),
                        r.transform(),
                        r.spread_method(),
                        (
                            FunctionShadingType::Radial,
                            vec![r.fx(), r.fy(), r.fr().get(), r.cx(), r.cy(), r.r().get()],
                        ),
                    ),
                    _ => unreachable!(),
                };
                let periods = Periods::cover(spread, &kind, gt, bbox);
                let shading = self.shading(stops, &kind, periods, false);
                let pattern = self.next();
                self.pdf
                    .shading_pattern(pattern)
                    .shading_ref(shading)
                    .matrix(matrix(at.ts.pre_concat(gt)));
                cv.set_pattern(pattern, stroke);
                if translucent_stops(stops) {
                    let alpha = self.shading(stops, &kind, periods, true);
                    let form = self.sub_form(bbox, |_, inner| {
                        inner.c.transform(matrix(gt));
                        inner.paint_shading(alpha);
                        Ok(())
                    })?;
                    alpha_mask = Some(form);
                }
            }
            Paint::Pattern(pt) => {
                let pattern = self.tiling(pt, at)?;
                cv.set_pattern(pattern, stroke);
            }
        }
        match alpha_mask {
            Some(form) => {
                let r = self.next();
                let mut gs = self.pdf.ext_graphics(r);
                gs.non_stroking_alpha(f).stroking_alpha(s);
                gs.soft_mask().subtype(MaskType::Luminosity).group(form);
                gs.finish();
                cv.set_gs(r);
            }
            None if opacity < 1.0 => {
                let r = self.alpha_gs(f, s, usvg::BlendMode::Normal)?;
                cv.set_gs(r);
            }
            None => {}
        }
        Ok(())
    }

    /// An axial or radial shading over the stops' colours (or, with
    /// `alpha`, their opacities as gray), repeated over `periods` and
    /// padded past them.
    fn shading(
        &mut self,
        stops: &[usvg::Stop],
        geometry: &Geometry,
        periods: Option<Periods>,
        alpha: bool,
    ) -> Ref {
        let value = |s: &usvg::Stop| -> Vec<f32> {
            if alpha {
                vec![s.opacity().get()]
            } else {
                let c = s.color();
                [c.red, c.green, c.blue]
                    .map(|v| f32::from(v) / 255.0)
                    .to_vec()
            }
        };
        let mut points: Vec<(f32, Vec<f32>)> =
            stops.iter().map(|s| (s.offset().get(), value(s))).collect();
        if points.is_empty() {
            points.push((0.0, vec![0.0; if alpha { 1 } else { 3 }]));
        }
        if points[0].0 > 0.0 {
            points.insert(0, (0.0, points[0].1.clone()));
        }
        if points[points.len() - 1].0 < 1.0 {
            points.push((1.0, points[points.len() - 1].1.clone()));
        }
        let mut parts = Vec::with_capacity(points.len() - 1);
        for w in points.windows(2) {
            let r = self.next();
            self.pdf
                .exponential_function(r)
                .domain([0.0, 1.0])
                .c0(w[0].1.iter().copied())
                .c1(w[1].1.iter().copied())
                .n(1.0);
            parts.push(r);
        }
        let function = if parts.len() == 1 {
            parts[0]
        } else {
            let r = self.next();
            let bounds: Vec<f32> = points[1..points.len() - 1].iter().map(|p| p.0).collect();
            self.pdf
                .stitching_function(r)
                .domain([0.0, 1.0])
                .functions(parts.iter().copied())
                .bounds(bounds)
                .encode(parts.iter().flat_map(|_| [0.0, 1.0]));
            r
        };
        // One copy of the stops per period, every other one mirrored when
        // reflecting.
        let (function, domain, coords) = match periods {
            Some(p) => {
                let starts: Vec<f32> = (p.from as i32..p.to as i32).map(|n| n as f32).collect();
                let r = self.next();
                self.pdf
                    .stitching_function(r)
                    .domain([p.from, p.to])
                    .functions(starts.iter().map(|_| function))
                    .bounds(starts[1..].iter().copied())
                    .encode(starts.iter().flat_map(|&n| {
                        if p.reflect && (n as i32).rem_euclid(2) == 1 {
                            [1.0, 0.0]
                        } else {
                            [0.0, 1.0]
                        }
                    }));
                (r, [p.from, p.to], p.coords(geometry))
            }
            None => (function, [0.0, 1.0], geometry.1.clone()),
        };
        let id = self.next();
        let mut sh = self.pdf.function_shading(id);
        sh.shading_type(geometry.0);
        if alpha {
            sh.color_space().device_gray();
        } else {
            sh.color_space().device_rgb();
        }
        if periods.is_some() {
            // Axial and radial shadings take a 2-element `/Domain`.
            sh.insert(Name(b"Domain")).array().items(domain);
        }
        sh.function(function)
            .coords(coords.iter().map(|&v| number(v)))
            .extend([true, true]);
        sh.finish();
        id
    }

    fn tiling(&mut self, pt: &usvg::Pattern, at: Scope) -> Result<Ref, InkoxideError> {
        let r = pt.rect();
        let mut cv = Canvas::new();
        self.group(&mut cv, pt.root(), Scope::form(at.depth + 1, false))?;
        let data = super::deflate(
            &super::content_bytes(std::mem::replace(&mut cv.c, Content::new()), self.profile)?,
            flate::LEVEL,
        )?;
        let id = self.next();
        self.out.tiling_pattern(self.pdf, id, &data, |t| {
            t.filter(Filter::FlateDecode);
            t.tiling_type(TilingType::ConstantSpacing)
                .paint_type(PaintType::Colored)
                .bbox(bounded(Rect::new(0.0, 0.0, r.width(), r.height())))
                .x_step(number(r.width()))
                .y_step(number(r.height()))
                .matrix(matrix(
                    at.ts.pre_concat(pt.transform()).pre_translate(r.x(), r.y()),
                ));
            cv.write_resources(t.resources());
        });
        Ok(id)
    }

    /// Writes an embedded raster image the first time its content is seen.
    fn raster(&mut self, data: &Arc<Vec<u8>>) -> Result<Option<Ref>, InkoxideError> {
        let key = ContentKey::new(data.clone());
        if let Some(&r) = self.images.get(&key) {
            return Ok(r);
        }
        let r = match crate::images::decode_embedded(data) {
            Ok(decoded) => Some(super::image::write_image(
                self.pdf,
                self.alloc,
                self.out,
                &decoded,
                self.profile,
                None,
            )?),
            Err(_) => None,
        };
        self.images.insert(key, r);
        Ok(r)
    }

    fn image(&mut self, cv: &mut Canvas, img: &usvg::Image, at: Scope) -> Res {
        if !img.is_visible() {
            return Ok(());
        }
        let size = img.size();
        let (w, h) = (number(size.width()), number(size.height()));
        match img.kind() {
            usvg::ImageKind::JPEG(data)
            | usvg::ImageKind::PNG(data)
            | usvg::ImageKind::GIF(data)
            | usvg::ImageKind::WEBP(data) => {
                // An image that cannot be decoded is left out, like
                // other formats.
                let Some(r) = self.raster(data)? else {
                    return Ok(());
                };
                cv.c.save_state();
                cv.c.transform([w, 0.0, 0.0, -h, 0.0, h]);
                cv.draw_xobject(r);
                cv.c.restore_state();
            }
            usvg::ImageKind::SVG(tree) => {
                let inner = tree.size();
                let scale = Transform::from_scale(w / inner.width(), h / inner.height());
                cv.c.save_state();
                cv.c.rect(0.0, 0.0, w, h);
                cv.c.clip_nonzero();
                cv.c.end_path();
                cv.c.transform(matrix(scale));
                let inner = Scope {
                    ts: at.ts.pre_concat(scale),
                    ..at.deeper()
                };
                self.group(cv, tree.root(), inner)?;
                cv.c.restore_state();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use pdf_writer::Pdf;

    use super::{Out, Profile, RefAlloc};
    use crate::images::{ImageBody, ImageCrateDecoder, ImageDecoder};

    /// The PDF objects written for `svg`.
    fn written(svg: &str) -> Vec<u8> {
        let ImageBody::Svg { tree, .. } = ImageCrateDecoder.decode(svg.as_bytes()).unwrap().body
        else {
            panic!("not an SVG");
        };
        let mut pdf = Pdf::new();
        super::write_svg(
            &mut pdf,
            &mut RefAlloc(0),
            &mut Out::plain(),
            (&tree, tree.size()),
            Profile::default(),
            None,
        )
        .unwrap();
        pdf.finish()
    }

    /// The objects written for `svg` as text, its streams inflated.
    fn written_text(svg: &str) -> String {
        inflated(&written(svg))
    }

    /// `out` as text, its streams inflated.
    fn inflated(out: &[u8]) -> String {
        let mut text = String::new();
        let mut rest = out;
        while let Some(i) = rest.windows(7).position(|w| w == b"stream\n") {
            text.push_str(&String::from_utf8_lossy(&rest[..i]));
            let body = &rest[i + 7..];
            let end = body.windows(9).position(|w| w == b"endstream").unwrap();
            let mut data = Vec::new();
            // Streams written uncompressed are kept as they are.
            if std::io::Read::read_to_end(
                &mut flate2::read::ZlibDecoder::new(&body[..end]),
                &mut data,
            )
            .is_err()
            {
                data = body[..end].to_vec();
            }
            text.push_str(&String::from_utf8_lossy(&data));
            rest = &body[end + b"endstream".len()..];
        }
        text + &String::from_utf8_lossy(rest)
    }

    /// `bytes` as a `data:` URL.
    fn url(mime: &str, bytes: &[u8]) -> String {
        let mut url = format!("data:{mime},");
        for b in bytes {
            let _ = write!(url, "%{b:02X}");
        }
        url
    }

    fn svg(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">{body}</svg>"#)
    }

    /// Image `XObject`s written for an SVG drawing `copies` copies of a PNG.
    fn images_written(copies: usize) -> usize {
        let png = url(
            "image/png",
            &crate::test_fixtures::fixture("images/logo.png"),
        );
        let out = written(&svg(&format!(
            r#"<defs><image id="i" width="4" height="4" href="{png}"/></defs>{}"#,
            r##"<use href="#i"/>"##.repeat(copies)
        )));
        out.windows(15).filter(|w| w == b"/Subtype /Image").count()
    }

    /// A document showing `svg` (with Inter Regular and Bold declared,
    /// and a line of text in Regular), rendered and inflated.
    fn rendered(svg: &str, pdf_a: bool) -> String {
        let doc = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"},
              {"id":"Inter-Bold","family":"Inter","weight":700}],
            "pages":[{"size":{"width":200,"height":200},"children":[
              {"type":"text","runs":[{"text":"Page","font":"Inter-Regular","size":10}]},
              {"type":"image","src":"svg"}]}]}"#;
        let fonts = crate::test_fixtures::fonts(&["Inter-Regular", "Inter-Bold"]);
        let mut images = crate::ImageStore::new();
        images.insert("svg", svg.as_bytes().to_vec());
        let opts = crate::RenderOptions::default()
            .with_compress(false)
            .with_pdf_a(pdf_a);
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &opts,
            &fonts,
            &images,
        )
        .unwrap();
        inflated(&pdf)
    }

    #[test]
    fn draws_text_as_pdf_text_with_its_fonts() {
        let out = rendered(
            &svg(r#"<text x="5" y="20" font-size="10" font-weight="bold" fill="red">Hi</text>"#),
            false,
        );
        // One run in a text object, with the probe's fill.
        assert!(out.contains("1 0 0 rg"), "{out}");
        assert!(out.contains("BT\n/F0 1 Tf\n10 0 0 -10 5 20 Tm\n["), "{out}");
        assert!(out.contains("] TJ\nET"), "{out}");
        assert!(out.contains("/Font <<\n      /F0"), "{out}");
        // Inter Bold, used only in the SVG, is embedded, with its glyphs
        // mapped to text.
        assert_eq!(out.matches("/FontFile2").count(), 2, "{out}");
        assert_eq!(out.matches("/ToUnicode").count(), 2, "{out}");
        assert!(out.contains("<0048>") && out.contains("<0069>"), "{out}");
        // The probe itself draws nothing.
        assert!(!out.contains(" re\nf"), "{out}");
    }

    /// Renders `svg` uncompressed after a line of page text, with Inter
    /// and `fallback` as the fallback font, or with the document's
    /// `emojiImages` mapping 😀 to a raster image and 🇫🇷 to the SVG itself.
    fn rendered_emoji(
        svg: &str,
        fallback: Option<&str>,
        profile: Option<crate::PdfALevel>,
    ) -> Result<String, crate::InkoxideError> {
        let (font, emoji) = match fallback {
            Some(f) => (
                format!(r#",{{"id":"{f}","family":"E"}}],"fallbackFonts":["{f}"]"#),
                String::new(),
            ),
            None => (
                "]".to_string(),
                r#","emojiImages":{"😀":"logo","🇫🇷":"svg"}"#.to_string(),
            ),
        };
        let doc = format!(
            r#"{{"version":1,"fonts":[{{"id":"Inter-Regular","family":"Inter"}}{font}{emoji},
                "pages":[{{"size":{{"width":200,"height":200}},"children":[
                  {{"type":"text","runs":[{{"text":"Page","font":"Inter-Regular","size":10}}]}},
                  {{"type":"image","src":"svg"}}]}}]}}"#
        );
        let fonts =
            crate::test_fixtures::fonts(&["Inter-Regular", fallback.unwrap_or("Inter-Bold")]);
        let mut images = crate::ImageStore::new();
        images.insert("svg", svg.as_bytes().to_vec());
        images.insert("logo", crate::test_fixtures::fixture("images/logo.png"));
        let mut opts = crate::RenderOptions::default().with_compress(false);
        if let Some(level) = profile {
            opts = opts.with_pdf_a_level(level);
        }
        let pdf = crate::render(
            doc.as_bytes(),
            crate::InputFormat::Json,
            &opts,
            &fonts,
            &images,
        )?;
        Ok(inflated(&pdf))
    }

    /// The `ActualText` of `text`, as the writer encodes it.
    fn actual_text(text: &str) -> String {
        let bytes = super::super::out::text_bytes(text);
        let hex = bytes.iter().fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02X}");
            hex
        });
        format!("/ActualText <{hex}>")
    }

    /// Colour glyphs in SVG text are images, as in page text: in a span
    /// with their text and an invisible space, inside the SVG's form. A
    /// font without outlines (CBDT) is never embedded.
    #[test]
    fn draws_colour_glyphs_of_svg_text_as_images() {
        let body = svg(r#"<text x="5" y="20" font-size="10">Hi 😀</text>"#);
        for font in ["EmojiTest-COLR", "EmojiTest-CBDT"] {
            let out = rendered_emoji(&body, Some(font), None).unwrap();
            assert!(out.contains(&actual_text("😀")), "{font}: {out}");
            assert!(out.contains("/Span <<"), "{font}");
            assert!(out.contains(" Do\n"), "{font}");
            assert!(out.contains("3 Tr"), "{font}");
            let embedded = out.contains(&font.replace('-', ""));
            assert_eq!(embedded, font == "EmojiTest-COLR", "{font}");
        }
    }

    /// Emoji the document maps to images are those images in SVG text, in
    /// spans with their text, where a missing glyph used to be: an SVG with
    /// text of its own is left out there, so no form draws itself, but its
    /// text is kept.
    #[test]
    fn draws_emoji_images_in_svg_text() {
        let body = svg(r#"<text x="5" y="20" font-size="10">a😀b🇫🇷</text>"#);
        let out = rendered_emoji(&body, None, None).unwrap();
        assert!(out.contains(&actual_text("😀")), "{out}");
        assert!(out.contains(&actual_text("🇫🇷")), "{out}");
        // The page draws the SVG, whose form draws the PNG, not itself.
        assert_eq!(out.matches("/Subtype /Form").count(), 1, "{out}");
        assert_eq!(out.matches(" Do\n").count(), 2, "{out}");
    }

    /// Under PDF/A-1b, a colour glyph that needs transparency in the text
    /// of an SVG fails the render at the image, as in page text.
    #[test]
    fn pdf_a_1b_rejects_colour_glyphs_of_svg_text_that_need_transparency() {
        let body = svg(r#"<text x="5" y="20" font-size="10">a 😀</text>"#);
        let err = rendered_emoji(&body, Some("EmojiTest-CBDT"), Some(crate::PdfALevel::A1b));
        let Err(crate::InkoxideError::Validation(e)) = err else {
            panic!("rendered: {err:?}");
        };
        assert_eq!(e.code, crate::ValidationCode::PdfaConflict);
        assert_eq!(e.path, "pages[0].children[1].src");
        assert!(e.message.contains("an alpha channel"), "{}", e.message);
    }

    /// An SVG without a size takes it from what it draws, which leaves its
    /// text out: the same with any fonts, and the size the layout gave it.
    #[test]
    fn an_svg_without_a_size_keeps_it_with_its_text() {
        let out = rendered(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/>
                 <text x="0" y="50" font-size="40">wide text</text></svg>"#,
            false,
        );
        assert!(out.contains("/BBox [0 0 10 10]"), "{out}");
        assert!(out.contains("/Matrix [0.1 0 0 -0.1 0 1]"), "{out}");
        assert!(out.contains("] TJ"), "{out}");
    }

    #[test]
    fn strokes_and_paints_text_with_gradients() {
        let out = rendered(
            &svg(
                r##"<linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient>
                   <text x="5" y="20" fill="url(#g)" stroke="#00ff00" stroke-width="0.5" paint-order="stroke">Hi</text>"##,
            ),
            false,
        );
        let stroke = out.find("1 Tr").expect("no stroked text");
        let pattern = out.find("/Pattern cs").expect("no gradient fill");
        // `paint-order: stroke` strokes first.
        assert!(stroke < pattern, "{out}");
        assert!(out.contains("0 1 0 RG"), "{out}");
    }

    #[test]
    fn fills_text_in_clip_paths_and_draws_decorations() {
        let out = rendered(
            &svg(
                r#"<clipPath id="c"><text x="5" y="40" font-size="30">Hi</text></clipPath>
                   <rect width="100" height="50" fill="blue" clip-path="url(#c)"/>
                   <text x="5" y="20" text-decoration="underline line-through">ab</text>"#,
            ),
            false,
        );
        // The clip path is a soft mask that fills the text.
        assert!(out.contains("/SMask"), "{out}");
        assert!(out.contains("0 g\n"), "{out}");
        assert_eq!(out.matches("] TJ").count(), 3, "{out}");
        // Underline right before the glyphs, line-through right after.
        let tm = out.find("16 0 0 -16 5 20 Tm").unwrap();
        let before = &out[..tm];
        assert!(before.ends_with("h\nf\nBT\n/F0 1 Tf\n"), "{before}");
        let after = &out[tm..];
        let et = after.find("ET\n").unwrap();
        assert!(
            after[et..].starts_with("ET\n") && after[et + 3..].contains("h\nf\n"),
            "{after}"
        );
    }

    #[test]
    fn leaves_notdef_out_of_pdf_a() {
        // Inter has no glyph for Thai.
        let svg = svg("<text x='5' y='20'>\u{0E01}</text>");
        assert!(rendered(&svg, false).contains("] TJ"));
        let a = rendered(&svg, true);
        assert_eq!(a.matches("] TJ").count(), 1, "only the page's text: {a}");
    }

    #[test]
    fn the_transparency_scan_agrees_with_the_writer() {
        let logo = url(
            "image/png",
            &crate::test_fixtures::fixture("images/logo.png"),
        );
        let photo = url(
            "image/jpeg",
            &crate::test_fixtures::fixture("images/photo.jpg"),
        );
        let veil = url(
            "image/svg+xml",
            svg(r#"<rect width="9" height="9" fill-opacity="0.5"/>"#).as_bytes(),
        );
        let cases = [
            (r#"<rect width="9" height="9" fill="red"/>"#.to_string(), None),
            (r#"<rect width="9" height="9" opacity="0.5"/>"#.into(), Some("group opacity")),
            (r#"<rect width="9" height="9" fill-opacity="0.5"/>"#.into(), Some("fill or stroke opacity")),
            (
                r#"<rect width="9" height="9" stroke="red" stroke-opacity="0.5"/>"#.into(),
                Some("fill or stroke opacity"),
            ),
            (
                r#"<rect width="9" height="9" style="mix-blend-mode:multiply"/>"#.into(),
                Some("a blend mode"),
            ),
            (r#"<g style="isolation:isolate"><rect width="9" height="9"/></g>"#.into(), Some("an isolated group")),
            (
                r#"<mask id="m"><rect width="5" height="5" fill="white"/></mask><rect width="9" height="9" mask="url(#m)"/>"#.into(),
                Some("a mask"),
            ),
            (
                r#"<clipPath id="c"><rect width="5" height="5"/></clipPath><rect width="9" height="9" clip-path="url(#c)"/>"#.into(),
                None,
            ),
            (
                r#"<clipPath id="c"><rect width="5" height="5"/><circle r="3"/></clipPath><rect width="9" height="9" clip-path="url(#c)"/>"#.into(),
                Some("a clip path other than a single path"),
            ),
            (
                r#"<linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient><rect width="9" height="9" fill="url(#g)"/>"#.into(),
                None,
            ),
            (
                r#"<linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-opacity="0.5"/></linearGradient><rect width="9" height="9" fill="url(#g)"/>"#.into(),
                Some("a gradient with stop opacity"),
            ),
            (
                r#"<pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2" opacity="0.5"/></pattern><rect width="9" height="9" fill="url(#p)"/>"#.into(),
                Some("group opacity"),
            ),
            (format!(r#"<image width="9" height="9" href="{photo}"/>"#), None),
            (format!(r#"<image width="9" height="9" href="{logo}"/>"#), Some("an alpha channel")),
            (format!(r#"<image width="9" height="9" href="{veil}"/>"#), Some("fill or stroke opacity")),
            // Inside a clip path, only the outline counts.
            (
                r#"<clipPath id="c"><rect width="5" height="5" fill-opacity="0.5"/></clipPath><rect width="9" height="9" clip-path="url(#c)"/>"#.into(),
                None,
            ),
        ];
        let one_b = Profile::new(Some(crate::PdfALevel::A1b));
        for (body, want) in cases {
            let ImageBody::Svg { tree, .. } = ImageCrateDecoder
                .decode(svg(&body).as_bytes())
                .unwrap()
                .body
            else {
                panic!("not an SVG");
            };
            assert_eq!(super::transparency(&tree, None), want, "{body}");
            // The writer refuses exactly what the scan finds.
            let refused = super::write_svg(
                &mut Pdf::new(),
                &mut RefAlloc(0),
                &mut Out::plain(),
                (&tree, tree.size()),
                one_b,
                None,
            );
            assert_eq!(refused.is_err(), want.is_some(), "{body}");
            super::write_svg(
                &mut Pdf::new(),
                &mut RefAlloc(0),
                &mut Out::plain(),
                (&tree, tree.size()),
                Profile::default(),
                None,
            )
            .unwrap();
        }
    }

    #[test]
    fn writes_each_embedded_image_once() {
        let once = images_written(1);
        assert!(once > 0);
        assert_eq!(images_written(5), once);
    }

    #[test]
    fn draws_embedded_gif_and_webp_images() {
        for (mime, file) in [
            ("image/gif", "anim.gif"),
            ("image/webp", "badge-lossless.webp"),
            ("", "photo-lossy.webp"),
        ] {
            let data = url(
                mime,
                &crate::test_fixtures::fixture(&format!("images/{file}")),
            );
            let out = written(&svg(&format!(
                r#"<image width="40" height="40" href="{data}"/>"#
            )));
            assert!(out.windows(15).any(|w| w == b"/Subtype /Image"), "{file}");
        }
    }

    #[test]
    fn draws_filtered_elements_without_the_filter() {
        let text = written_text(&svg(
            r##"<filter id="s"><feDropShadow dx="2" dy="2"/></filter>
               <rect width="10" height="10" fill="#00ff00" filter="url(#s)"/>"##,
        ));
        assert!(text.contains("0 1 0 rg"), "{text}");
    }

    #[test]
    fn repeats_and_reflects_gradients_over_what_they_paint() {
        let gradient = |spread: &str| {
            String::from_utf8_lossy(&written(&svg(&format!(
                r#"<linearGradient id="g" x2="0.2" spreadMethod="{spread}">
                     <stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/>
                   </linearGradient>
                   <rect width="100" height="50" fill="url(#g)"/>"#
            ))))
            .into_owned()
        };
        // A fifth of the width per period: five periods.
        let repeat = gradient("repeat");
        assert!(repeat.contains("/Domain [0 5]"), "{repeat}");
        assert!(repeat.contains("/Bounds [1 2 3 4]"), "{repeat}");
        assert!(repeat.contains("/Encode [0 1 0 1 0 1 0 1 0 1]"), "{repeat}");
        let reflect = gradient("reflect");
        assert!(
            reflect.contains("/Encode [0 1 1 0 0 1 1 0 0 1]"),
            "{reflect}"
        );
        // pad: one ramp, extended.
        let pad = gradient("pad");
        assert!(
            !pad.contains("/Bounds") && !pad.contains("/Domain [0 5]"),
            "{pad}"
        );
    }

    #[test]
    fn repeats_radial_gradients_out_to_the_far_corner() {
        let out = String::from_utf8_lossy(&written(&svg(
            r#"<radialGradient id="g" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="10" spreadMethod="repeat">
                 <stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/>
               </radialGradient>
               <rect width="100" height="50" fill="url(#g)"/>"#,
        )))
        .into_owned();
        // The far corner is √(100² + 50²) ≈ 111.8 from the centre: 12
        // periods, the last circle of radius 120.
        assert!(out.contains("/Domain [0 12]"), "{out}");
        assert!(out.contains("/Coords [0 0 0 0 0 120]"), "{out}");
    }
}
