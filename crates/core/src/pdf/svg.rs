//! SVG → PDF: a `usvg` tree becomes a Form `XObject` with vector content.
//!
//! - Paths keep their geometry; quadratic segments become cubics.
//! - Solid paints are RGB colours; gradients are axial/radial shading
//!   patterns, with a luminosity soft mask when stop opacities vary;
//!   patterns are coloured tiling patterns.
//! - Group opacity, blend modes, masks and clip paths that are not a
//!   single path go through transparency groups and soft masks. A clip
//!   path that is one path becomes a plain `W n` clip.
//! - Embedded JPEG/PNG images reuse the raster image writer, once per
//!   distinct image however many copies draw it; nested SVGs are drawn
//!   inline.
//! - Filters are not supported: filtered elements are left out.
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

use super::{finite, RefAlloc};
use crate::InkoxideError;

/// Most path segments one SVG may emit, copies included.
const MAX_SEGMENTS: usize = 5_000_000;
/// Deepest group nesting followed (usvg trees from checked input stay far
/// below this).
const MAX_DEPTH: u32 = 256;
const FLATE_LEVEL: u32 = 6;

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
    s.group(&mut cv, tree.root(), Transform::identity(), 0, false)?;
    let id = s.alloc.next();
    s.write_form(
        id,
        cv,
        Rect::new(0.0, 0.0, w, h),
        Some([1.0 / w, 0.0, 0.0, -1.0 / h, 0.0, 1.0].map(finite)),
        false,
    )?;
    Ok(id)
}

/// A content stream under construction and the resources it names.
struct Canvas {
    c: Content,
    gs: Vec<Ref>,
    xobjects: Vec<Ref>,
    patterns: Vec<Ref>,
    shadings: Vec<Ref>,
}

fn slot(list: &mut Vec<Ref>, r: Ref) -> usize {
    if let Some(i) = list.iter().position(|&x| x == r) {
        i
    } else {
        list.push(r);
        list.len() - 1
    }
}

impl Canvas {
    fn new() -> Self {
        Self {
            c: Content::new(),
            gs: Vec::new(),
            xobjects: Vec::new(),
            patterns: Vec::new(),
            shadings: Vec::new(),
        }
    }

    fn set_gs(&mut self, r: Ref) {
        let name = format!("G{}", slot(&mut self.gs, r));
        self.c.set_parameters(Name(name.as_bytes()));
    }

    fn draw_xobject(&mut self, r: Ref) {
        let name = format!("X{}", slot(&mut self.xobjects, r));
        self.c.x_object(Name(name.as_bytes()));
    }

    fn set_pattern(&mut self, r: Ref, stroke: bool) {
        let name = format!("P{}", slot(&mut self.patterns, r));
        if stroke {
            self.c.set_stroke_color_space(ColorSpaceOperand::Pattern);
            self.c.set_stroke_pattern(None, Name(name.as_bytes()));
        } else {
            self.c.set_fill_color_space(ColorSpaceOperand::Pattern);
            self.c.set_fill_pattern(None, Name(name.as_bytes()));
        }
    }

    fn paint_shading(&mut self, r: Ref) {
        let name = format!("S{}", slot(&mut self.shadings, r));
        self.c.shading(Name(name.as_bytes()));
    }

    fn write_resources(&self, mut res: pdf_writer::writers::Resources<'_>) {
        let lists: [(&[Ref], &str); 4] = [
            (&self.gs, "G"),
            (&self.xobjects, "X"),
            (&self.patterns, "P"),
            (&self.shadings, "S"),
        ];
        for (list, prefix) in lists {
            if list.is_empty() {
                continue;
            }
            let mut d = match prefix {
                "G" => res.ext_g_states(),
                "X" => res.x_objects(),
                "P" => res.patterns(),
                _ => res.shadings(),
            };
            for (i, r) in list.iter().enumerate() {
                d.pair(Name(format!("{prefix}{i}").as_bytes()), *r);
            }
        }
    }
}

/// A transform as a PDF matrix. Transforms compound (a tiny `viewBox`
/// scaled up, nested scales), so they can overflow `f32`; see
/// [`super::finite`].
fn matrix(t: Transform) -> [f32; 6] {
    [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(finite)
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

struct Svg<'a> {
    pdf: &'a mut Pdf,
    alloc: &'a mut RefAlloc,
    /// Shared graphics states for (fill alpha, stroke alpha, blend mode).
    alphas: HashMap<(u16, u16, u8), Ref>,
    /// Embedded raster images by content hash: the bytes (to rule out
    /// collisions) and the image `XObject`, `None` if it did not decode.
    images: HashMap<u64, (Arc<Vec<u8>>, Option<Ref>)>,
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
            &std::mem::replace(&mut cv.c, Content::new()).finish(),
            FLATE_LEVEL,
        )?;
        let mut f = self.pdf.form_xobject(id, &data);
        f.filter(Filter::FlateDecode);
        f.bbox(bbox);
        if let Some(m) = m {
            f.matrix(m);
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
        let key = (
            (fill.clamp(0.0, 1.0) * 1000.0).round() as u16,
            (stroke.clamp(0.0, 1.0) * 1000.0).round() as u16,
            blend as u8,
        );
        if let Some(&r) = self.alphas.get(&key) {
            return r;
        }
        let r = self.next();
        let mut gs = self.pdf.ext_graphics(r);
        gs.non_stroking_alpha(f32::from(key.0) / 1000.0)
            .stroking_alpha(f32::from(key.1) / 1000.0);
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
        let mut last = Point::zero();
        let mut start = Point::zero();
        for seg in path.segments() {
            match seg {
                PathSegment::MoveTo(p) => {
                    c.move_to(p.x, p.y);
                    (last, start) = (p, p);
                }
                PathSegment::LineTo(p) => {
                    c.line_to(p.x, p.y);
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
                    c.cubic_to(c1.0, c1.1, c2.0, c2.1, p.x, p.y);
                    last = p;
                }
                PathSegment::CubicTo(p1, p2, p) => {
                    c.cubic_to(p1.x, p1.y, p2.x, p2.y, p.x, p.y);
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

    /// Draws a group. In `clip` mode everything is filled opaque black
    /// (the content of an alpha soft mask built from a clip path).
    fn group(&mut self, cv: &mut Canvas, g: &Group, ts: Transform, depth: u32, clip: bool) -> Res {
        if depth > MAX_DEPTH || !g.filters().is_empty() {
            return Ok(());
        }
        let ts = ts.pre_concat(g.transform());
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
            self.children(cv, g, ts, depth, clip)?;
        } else {
            self.layered(cv, &masks, last, bbox, g, depth, clip)?;
        }
        cv.c.restore_state();
        Ok(())
    }

    /// Applies `masks[0]`, then draws the rest of the layers as a
    /// transparency group under it; the innermost group holds `g`'s
    /// children and is painted with `last` (opacity, blend mode).
    #[allow(clippy::too_many_arguments)]
    fn layered(
        &mut self,
        cv: &mut Canvas,
        masks: &[Ref],
        last: Option<Ref>,
        bbox: Rect,
        g: &Group,
        depth: u32,
        clip: bool,
    ) -> Res {
        let form = if let Some((&m, rest)) = masks.split_first() {
            cv.set_gs(m);
            self.sub_form(bbox, |s, inner| {
                s.layered(inner, rest, last, bbox, g, depth, clip)
            })?
        } else {
            if let Some(l) = last {
                cv.set_gs(l);
            }
            self.sub_form(bbox, |s, inner| {
                s.children(inner, g, Transform::identity(), depth, clip)
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
            s.group(inner, cp.root(), cp.transform(), depth + 1, true)?;
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
            inner.c.rect(r.x(), r.y(), r.width(), r.height());
            inner.c.clip_nonzero();
            inner.c.end_path();
            s.group(inner, m.root(), Transform::identity(), depth + 1, false)
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

    fn children(
        &mut self,
        cv: &mut Canvas,
        g: &Group,
        ts: Transform,
        depth: u32,
        clip: bool,
    ) -> Res {
        for n in g.children() {
            match n {
                Node::Group(child) => self.group(cv, child, ts, depth + 1, clip)?,
                Node::Path(p) if clip => {
                    if let (true, Some(fill)) = (p.is_visible(), p.fill()) {
                        self.emit_path(&mut cv.c, p.data())?;
                        match fill.rule() {
                            usvg::FillRule::NonZero => cv.c.fill_nonzero(),
                            usvg::FillRule::EvenOdd => cv.c.fill_even_odd(),
                        };
                    }
                }
                Node::Path(p) => self.path(cv, p, ts, depth)?,
                Node::Image(img) if !clip => self.image(cv, img, ts, depth)?,
                Node::Image(_) => {}
                Node::Text(t) => self.group(cv, t.flattened(), ts, depth + 1, clip)?,
            }
        }
        Ok(())
    }

    fn path(&mut self, cv: &mut Canvas, p: &usvg::Path, ts: Transform, depth: u32) -> Res {
        if !p.is_visible() {
            return Ok(());
        }
        let area = p.stroke_bounding_box();
        let bbox = Rect::new(area.left(), area.top(), area.right(), area.bottom());
        let fill = |s: &mut Self, cv: &mut Canvas| -> Res {
            let Some(f) = p.fill() else { return Ok(()) };
            cv.c.save_state();
            s.set_paint(cv, f.paint(), f.opacity().get(), false, ts, bbox, depth)?;
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
            s.set_paint(cv, st.paint(), st.opacity().get(), true, ts, bbox, depth)?;
            cv.c.set_line_width(finite(st.width().get()));
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
            cv.c.set_miter_limit(finite(st.miterlimit().get()));
            if let Some(d) = st.dasharray() {
                cv.c.set_dash_pattern(d.iter().map(|&v| finite(v)), finite(st.dashoffset()));
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
    #[allow(clippy::too_many_arguments)]
    fn set_paint(
        &mut self,
        cv: &mut Canvas,
        paint: &Paint,
        opacity: f32,
        stroke: bool,
        ts: Transform,
        bbox: Rect,
        depth: u32,
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
                let (stops, gt, kind) = match paint {
                    Paint::LinearGradient(l) => (
                        l.stops(),
                        l.transform(),
                        (
                            FunctionShadingType::Axial,
                            vec![l.x1(), l.y1(), l.x2(), l.y2()],
                        ),
                    ),
                    Paint::RadialGradient(r) => (
                        r.stops(),
                        r.transform(),
                        (
                            FunctionShadingType::Radial,
                            vec![r.fx(), r.fy(), r.fr().get(), r.cx(), r.cy(), r.r().get()],
                        ),
                    ),
                    _ => unreachable!(),
                };
                let shading = self.shading(stops, &kind, false);
                let pattern = self.next();
                self.pdf
                    .shading_pattern(pattern)
                    .shading_ref(shading)
                    .matrix(matrix(ts.pre_concat(gt)));
                cv.set_pattern(pattern, stroke);
                if stops.iter().any(|s| s.opacity().get() < 1.0) {
                    let alpha = self.shading(stops, &kind, true);
                    let form = self.sub_form(bbox, |_, inner| {
                        inner.c.transform(matrix(gt));
                        inner.paint_shading(alpha);
                        Ok(())
                    })?;
                    alpha_mask = Some(form);
                }
            }
            Paint::Pattern(pt) => {
                let pattern = self.tiling(pt, ts, depth)?;
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
    /// `alpha`, their opacities as gray), padded at both ends.
    fn shading(
        &mut self,
        stops: &[usvg::Stop],
        (kind, coords): &(FunctionShadingType, Vec<f32>),
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
        let id = self.next();
        let mut sh = self.pdf.function_shading(id);
        sh.shading_type(*kind);
        if alpha {
            sh.color_space().device_gray();
        } else {
            sh.color_space().device_rgb();
        }
        sh.function(function)
            .coords(coords.iter().map(|&v| finite(v)))
            .extend([true, true]);
        sh.finish();
        id
    }

    fn tiling(
        &mut self,
        pt: &usvg::Pattern,
        ts: Transform,
        depth: u32,
    ) -> Result<Ref, InkoxideError> {
        let r = pt.rect();
        let mut cv = Canvas::new();
        self.group(&mut cv, pt.root(), Transform::identity(), depth + 1, false)?;
        let data = super::deflate(
            &std::mem::replace(&mut cv.c, Content::new()).finish(),
            FLATE_LEVEL,
        )?;
        let id = self.next();
        let mut t = self.pdf.tiling_pattern(id, &data);
        t.filter(Filter::FlateDecode);
        t.tiling_type(TilingType::ConstantSpacing)
            .paint_type(PaintType::Colored)
            .bbox(Rect::new(0.0, 0.0, r.width(), r.height()))
            .x_step(r.width())
            .y_step(r.height())
            .matrix(matrix(
                ts.pre_concat(pt.transform()).pre_translate(r.x(), r.y()),
            ));
        cv.write_resources(t.resources());
        Ok(id)
    }

    /// Writes an embedded raster image the first time its content is seen.
    fn raster(&mut self, data: &Arc<Vec<u8>>) -> Result<Option<Ref>, InkoxideError> {
        let hash = xxhash_rust::xxh3::xxh3_64(data);
        if let Some((bytes, r)) = self.images.get(&hash) {
            if bytes[..] == data[..] {
                return Ok(*r);
            }
        }
        let r = match crate::images::decode_embedded(data) {
            Ok(decoded) => Some(super::image::write_image(
                self.pdf, self.alloc, &decoded, self.pdf_a,
            )?),
            Err(_) => None,
        };
        self.images.entry(hash).or_insert_with(|| (data.clone(), r));
        Ok(r)
    }

    fn image(&mut self, cv: &mut Canvas, img: &usvg::Image, ts: Transform, depth: u32) -> Res {
        if !img.is_visible() {
            return Ok(());
        }
        let size = img.size();
        let (w, h) = (size.width(), size.height());
        match img.kind() {
            usvg::ImageKind::JPEG(data) | usvg::ImageKind::PNG(data) => {
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
                self.group(cv, tree.root(), ts.pre_concat(scale), depth + 1, false)?;
                cv.c.restore_state();
            }
            _ => {}
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

    /// Image `XObject`s written for an SVG drawing `copies` copies of a PNG.
    fn images_written(copies: usize) -> usize {
        let png = crate::test_fixtures::fixture("images/logo.png");
        let mut url = String::from("data:image/png,");
        for b in png {
            let _ = write!(url, "%{b:02X}");
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><defs><image id="i" width="4" height="4" href="{url}"/></defs>{}</svg>"#,
            r##"<use href="#i"/>"##.repeat(copies)
        );
        let ImageBody::Svg(tree) = ImageCrateDecoder.decode(svg.as_bytes()).unwrap().body else {
            panic!("not an SVG");
        };
        let mut pdf = Pdf::new();
        super::write_svg(&mut pdf, &mut RefAlloc(0), &tree, false).unwrap();
        let out = pdf.finish();
        out.windows(15).filter(|w| w == b"/Subtype /Image").count()
    }

    #[test]
    fn writes_each_embedded_image_once() {
        let once = images_written(1);
        assert!(once > 0);
        assert_eq!(images_written(5), once);
    }
}
