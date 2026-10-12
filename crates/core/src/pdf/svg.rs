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
//!
//! Patterns and shadings are positioned relative to the form they are used
//! in, so the transform from the current form's space is tracked (`ts`)
//! and reset for every new form.

use std::collections::HashMap;
use std::sync::Arc;

use pdf_writer::types::{
    BlendMode, ColorSpaceOperand, FunctionShadingType, LineCapStyle, LineJoinStyle, MaskType,
    PaintType, TilingType,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
use usvg::tiny_skia_path::{self, PathSegment, Point};
use usvg::{Group, Node, Paint, Transform};

use super::{alpha_key, key_alpha, number, RefAlloc};
use crate::images::ContentKey;
use crate::{flate, InkoxideError};

/// Most path segments one SVG may emit, copies included.
const MAX_SEGMENTS: usize = 5_000_000;
/// Deepest group nesting followed (usvg trees from checked input stay far
/// below this).
const MAX_DEPTH: u32 = 256;
/// Most periods of a repeated or reflected gradient written; past them it
/// pads. Each is an entry in a stitching function.
const MAX_PERIODS: f32 = 1000.0;

/// Writes `tree` as a Form `XObject` mapping the SVG viewport onto the unit
/// square (y up), like an image `XObject`.
pub(crate) fn write_svg(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    tree: &usvg::Tree,
    pdf_a: bool,
) -> Result<Ref, InkoxideError> {
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    let mut s = Svg {
        pdf,
        alloc,
        alphas: HashMap::new(),
        images: HashMap::new(),
        segments: 0,
        pdf_a,
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
}

impl ResKind {
    const ALL: [Self; 4] = [
        Self::GraphicsState,
        Self::XObject,
        Self::Pattern,
        Self::Shading,
    ];

    /// The resource names of this kind are the prefix and a number.
    fn prefix(self) -> &'static str {
        match self {
            Self::GraphicsState => "G",
            Self::XObject => "X",
            Self::Pattern => "P",
            Self::Shading => "S",
        }
    }

    /// The dictionary of `res` that holds resources of this kind.
    fn dict<'a>(self, res: &'a mut pdf_writer::writers::Resources<'_>) -> pdf_writer::Dict<'a> {
        match self {
            Self::GraphicsState => res.ext_g_states(),
            Self::XObject => res.x_objects(),
            Self::Pattern => res.patterns(),
            Self::Shading => res.shadings(),
        }
    }
}

/// A content stream under construction and the resources it names.
struct Canvas {
    c: Content,
    /// Resources by kind (see [`ResKind::ALL`]); a resource's position is
    /// the number in its name.
    res: [Vec<Ref>; 4],
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

/// A clip path that is exactly one filled path: its outline in the clip's
/// parent space and its rule.
fn simple_clip(clip: &usvg::ClipPath) -> Option<(tiny_skia_path::Path, usvg::FillRule)> {
    let root = clip.root();
    if clip.clip_path().is_some() || root.clip_path().is_some() || root.mask().is_some() {
        return None;
    }
    let [Node::Path(p)] = root.children() else {
        return None;
    };
    let rule = p.fill()?.rule();
    if !p.is_visible() {
        return None;
    }
    let ts = clip.transform().pre_concat(root.transform());
    let path = p.data().clone().transform(ts)?;
    Some((path, rule))
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
    /// Shared graphics states for (fill alpha, stroke alpha, blend mode).
    alphas: HashMap<(u16, u16, u8), Ref>,
    /// Embedded raster images by content: the image `XObject`, `None` if
    /// it did not decode.
    images: HashMap<ContentKey<Arc<Vec<u8>>>, Option<Ref>>,
    segments: usize,
    /// Passed on to embedded raster images.
    pdf_a: bool,
}

type Res = Result<(), InkoxideError>;

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
            &super::content_bytes(std::mem::replace(&mut cv.c, Content::new())),
            flate::LEVEL,
        )?;
        let mut f = self.pdf.form_xobject(id, &data);
        f.filter(Filter::FlateDecode);
        f.bbox(bounded(bbox));
        if let Some(m) = m {
            f.matrix(m.map(number));
        }
        if group {
            f.group().transparency().color_space().device_rgb();
        }
        cv.write_resources(f.resources());
        Ok(())
    }

    /// A transparency group form over `bbox` (current space) with content
    /// drawn by `draw`, whose `ts` starts at the identity.
    fn sub_form(
        &mut self,
        bbox: Rect,
        draw: impl FnOnce(&mut Self, &mut Canvas) -> Res,
    ) -> Result<Ref, InkoxideError> {
        let mut cv = Canvas::new();
        draw(self, &mut cv)?;
        let id = self.next();
        self.write_form(id, cv, bbox, None, true)?;
        Ok(id)
    }

    /// Graphics state with constant alphas and a blend mode (shared).
    fn alpha_gs(&mut self, fill: f32, stroke: f32, blend: usvg::BlendMode) -> Ref {
        let key = (alpha_key(fill), alpha_key(stroke), blend as u8);
        if let Some(&r) = self.alphas.get(&key) {
            return r;
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
        r
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
            return Err(InkoxideError::Image(format!(
                "SVG is too complex: more than {MAX_SEGMENTS} path segments"
            )));
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
            match simple_clip(cp) {
                Some((path, rule)) => {
                    self.emit_path(&mut cv.c, &path)?;
                    match rule {
                        usvg::FillRule::NonZero => cv.c.clip_nonzero(),
                        usvg::FillRule::EvenOdd => cv.c.clip_even_odd(),
                    };
                    cv.c.end_path();
                }
                None if cp.root().children().is_empty() && cp.clip_path().is_none() => {
                    // Nothing is inside the clip.
                    cv.c.restore_state();
                    return Ok(());
                }
                None => self.clip_masks(cp, bbox, depth, &mut masks)?,
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
        let last = (opacity < 1.0 || blend != usvg::BlendMode::Normal)
            .then(|| self.alpha_gs(opacity, opacity, blend));
        if masks.is_empty() && last.is_none() && (clip || !g.isolate()) {
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
            s.set_paint(cv, st.paint(), st.opacity().get(), true, bbox, at)?;
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
                if stops.iter().any(|s| s.opacity().get() < 1.0) {
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
                let r = self.alpha_gs(f, s, usvg::BlendMode::Normal);
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
            &super::content_bytes(std::mem::replace(&mut cv.c, Content::new())),
            flate::LEVEL,
        )?;
        let id = self.next();
        let mut t = self.pdf.tiling_pattern(id, &data);
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
                self.pdf, self.alloc, &decoded, self.pdf_a,
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

    use super::RefAlloc;
    use crate::images::{ImageBody, ImageCrateDecoder, ImageDecoder};

    /// The PDF objects written for `svg`.
    fn written(svg: &str) -> Vec<u8> {
        let ImageBody::Svg { tree, .. } = ImageCrateDecoder.decode(svg.as_bytes()).unwrap().body
        else {
            panic!("not an SVG");
        };
        let mut pdf = Pdf::new();
        super::write_svg(&mut pdf, &mut RefAlloc(0), &tree, false).unwrap();
        pdf.finish()
    }

    /// The objects written for `svg` as text, its streams inflated.
    fn written_text(svg: &str) -> String {
        let out = written(svg);
        let mut text = String::new();
        let mut rest = &out[..];
        while let Some(i) = rest.windows(7).position(|w| w == b"stream\n") {
            text.push_str(&String::from_utf8_lossy(&rest[..i]));
            let body = &rest[i + 7..];
            let end = body.windows(9).position(|w| w == b"endstream").unwrap();
            let mut data = Vec::new();
            std::io::Read::read_to_end(
                &mut flate2::read::ZlibDecoder::new(&body[..end]),
                &mut data,
            )
            .unwrap();
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
