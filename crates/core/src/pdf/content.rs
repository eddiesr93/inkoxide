//! Content streams: a page's display list as PDF operators, and what the
//! writer needs to know of the page besides.

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::types::{LineCapStyle, TextRenderingMode};
use pdf_writer::{Content, Name, Str};

use crate::fonts::{FontKey, FontSet};
use crate::geom::{apply, mul, Rect};
use crate::images::ImageKey;
use crate::paint::{FillRule, InvisibleGlyph, Op, PageContent, PathSeg, TextOp};
use crate::schema::Color;
use crate::tag::{Plan, StructId};
use crate::InkoxideError;

use super::{content_bytes, deflate, out, round_pt, round_to, EncodedPage, Profile};

/// A glyph closer than this (points) to where the previous glyph's advance
/// puts it needs no `TJ` adjustment.
pub(super) const KERN_EPS: f32 = 1e-3;

/// Encodes one page: its content stream, compressed at Flate level
/// `compression`, and the resources and areas the writer needs from it.
/// With `plan` the content is tagged.
pub(super) fn encode_page(
    page: &PageContent,
    fonts: &FontSet<'_>,
    profile: Profile,
    compression: Option<u32>,
    plan: Option<&Plan>,
) -> Result<EncodedPage, InkoxideError> {
    let mut glyphs: BTreeMap<FontKey, BTreeSet<u16>> = BTreeMap::new();
    let mut opacities = BTreeSet::new();
    let mut images = BTreeSet::new();
    let mut links = Vec::new();
    let mut fields = Vec::new();
    let mut icons = Vec::new();
    let mut owners = AnnotOwners::default();
    // The element whose content is drawn.
    let mut owner: Option<StructId> = None;
    // Links and widgets are areas on the page: they cover where the
    // transform in effect puts them.
    let mut ctm: Option<[f32; 6]> = None;
    let mut saved = Vec::new();
    for op in &page.ops {
        match op {
            Op::Tag(id) => owner = Some(*id),
            Op::EndTag => owner = None,
            Op::Save => saved.push(ctm),
            Op::Restore => ctm = saved.pop().flatten(),
            Op::Transform(m) => ctm = Some(ctm.map_or(*m, |c| mul(c, *m))),
            Op::Text(t) => {
                // The painter leaves out the text of a font without
                // outlines (`ColorGlyphs::push`): it is never embedded.
                debug_assert!(fonts.get(t.font).has_outlines(), "{}", fonts.get(t.font).id);
                glyphs
                    .entry(t.font)
                    .or_default()
                    .extend(t.glyphs.iter().map(|g| g.id));
            }
            Op::SetOpacity(a) => {
                opacities.insert(alpha_key(*a));
            }
            Op::Image { key, .. } => {
                images.insert(*key);
            }
            Op::InvisibleText(g) => {
                glyphs.entry(g.font).or_default().insert(g.glyph);
            }
            Op::Link {
                rect,
                target,
                owner: own,
            } => {
                links.push((ctm.map_or(*rect, |m| bounds(m, *rect)), target.clone()));
                owners.links.push(own.or(owner));
            }
            Op::Field(f) => {
                if let Some(font) = f.font {
                    let used = glyphs.entry(font).or_default();
                    for t in &f.text {
                        used.extend(t.glyphs.iter().map(|g| g.id));
                    }
                    used.extend(&f.option_glyphs);
                }
                let mut f = (**f).clone();
                f.ctm = ctm;
                fields.push(f);
                owners.fields.push(owner);
            }
            // An icon stays upright and the same size: only its corner
            // moves.
            Op::Icon(i) => {
                let mut i = (**i).clone();
                if let Some(m) = ctm {
                    (i.x, i.y) = apply(m, (i.x, i.y));
                }
                icons.push(i);
                owners.icons.push(owner);
            }
            _ => {}
        }
    }
    let (raw, mcids) = marked_content_stream(page, fonts, profile, plan)?;
    let tags = plan.map(|_| PageTags { mcids, owners });
    let stream = match compression {
        Some(level) => deflate(&raw, level)?,
        None => raw,
    };
    Ok(EncodedPage {
        width: page.width,
        height: page.height,
        stream,
        compressed: compression.is_some(),
        glyphs,
        opacities,
        images,
        links,
        fields,
        icons,
        tags,
    })
}

/// What a tagged page's structure needs: the element of each
/// marked-content sequence, by MCID, and of each annotation.
#[derive(Debug, Default)]
pub(crate) struct PageTags {
    pub mcids: Vec<StructId>,
    pub owners: AnnotOwners,
}

/// The element of each annotation of a page, in the order of `links`,
/// `fields` and `icons`; `None` for an annotation in decoration (a header),
/// which gets an element of its own.
#[derive(Debug, Default)]
pub(crate) struct AnnotOwners {
    pub links: Vec<Option<StructId>>,
    pub fields: Vec<Option<StructId>>,
    pub icons: Vec<Option<StructId>>,
}

/// Marked content in a tagged content stream: each element's content in a
/// sequence with an MCID, and everything else, drawn outside them, in
/// artifacts. Both are opened when something is drawn, so nothing marks an
/// empty sequence, and an artifact is closed before the save level it
/// started in ends.
struct Marker<'p> {
    plan: &'p Plan,
    /// The owner of each MCID so far.
    mcids: Vec<StructId>,
    /// The element whose content is drawn now, and whether its sequence is
    /// open.
    tag: Option<(StructId, bool)>,
    /// The save level an open artifact started at.
    artifact: Option<usize>,
    depth: usize,
}

impl Marker<'_> {
    fn tag(&mut self, c: &mut Content, id: StructId) {
        self.end_tag(c);
        self.tag = Some((id, false));
    }

    fn end_tag(&mut self, c: &mut Content) {
        if let Some((_, true)) = self.tag.take() {
            c.end_marked_content();
        }
    }

    /// Before an operator that draws: in the sequence of the element drawn,
    /// else in an artifact.
    fn draw(&mut self, c: &mut Content) {
        match self.tag {
            Some((id, false)) => {
                self.close_artifact(c);
                let name = self.plan.elems[id as usize].kind.name();
                c.begin_marked_content_with_properties(Name(name))
                    .properties()
                    .pair(Name(b"MCID"), self.mcids.len() as i32);
                self.mcids.push(id);
                self.tag = Some((id, true));
            }
            None if self.artifact.is_none() => {
                c.begin_marked_content(Name(b"Artifact"));
                self.artifact = Some(self.depth);
            }
            _ => {}
        }
    }

    fn close_artifact(&mut self, c: &mut Content) {
        if self.artifact.take().is_some() {
            c.end_marked_content();
        }
    }

    /// Before a `Q` that ends save level `self.depth`.
    fn restore(&mut self, c: &mut Content) {
        if self.artifact == Some(self.depth) {
            self.close_artifact(c);
        }
    }
}

/// The upright rectangle that `rect` covers once transformed by `m`.
pub(super) fn bounds(m: [f32; 6], rect: Rect) -> Rect {
    let Rect { x, y, w, h } = rect;
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)].map(|p| apply(m, p));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (px, py) in corners {
        (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Opacity quantised to 1/1000: the key of its shared graphics state.
pub(super) fn alpha_key(a: f32) -> u16 {
    (a.clamp(0.0, 1.0) * 1000.0).round() as u16
}

/// The opacity [`alpha_key`] quantised to `key`.
pub(super) fn key_alpha(key: u16) -> f32 {
    f32::from(key) / 1000.0
}

pub(super) fn gs_name(key: u16) -> String {
    format!("A{key}")
}

pub(super) fn image_name(key: ImageKey) -> String {
    format!("Im{key}")
}

pub(super) fn font_name(key: FontKey) -> String {
    format!("F{key}")
}

/// Text ops: one `TJ` per stretch of glyphs on the baseline, with kerning
/// adjustments wherever our positions differ from the font's advances.
/// With `skip_notdef` (PDF/A), `.notdef` glyphs are left out and the next
/// glyph keeps its position.
pub(super) fn show_text(c: &mut Content, t: &TextOp, fonts: &FontSet<'_>, skip_notdef: bool) {
    let font = fonts.get(t.font);
    let scale = font.scale(t.size);
    let size = round_pt(t.size);
    set_fill(c, &t.color);
    c.begin_text();
    c.set_font(Name(font_name(t.font).as_bytes()), size);
    let glyphs = &t.glyphs;
    let mut i = 0;
    while i < glyphs.len() {
        let g0 = glyphs[i];
        if skip_notdef && g0.id == 0 {
            i += 1;
            continue;
        }
        // Text space is y-up; the page is flipped, so flip back.
        c.set_text_matrix([
            1.0,
            0.0,
            0.0,
            -1.0,
            round_pt(t.x + g0.x),
            round_pt(t.y - g0.y_offset),
        ]);
        if g0.y_offset != 0.0 {
            c.show(Str(&g0.id.to_be_bytes()));
            i += 1;
            continue;
        }
        // Up to the next glyph moved up or down.
        let end = glyphs[i..]
            .iter()
            .position(|g| g.y_offset != 0.0)
            .map_or(glyphs.len(), |n| i + n);
        let run = glyphs[i..end]
            .iter()
            .filter(|g| !(skip_notdef && g.id == 0));
        let run = run.map(|g| (g.id, g.x, font.advance(g.id) * scale));
        show_kerned(c, g0.x, t.size, round_pt, run);
        i = end;
    }
    c.end_text();
}

/// Shows `glyphs` (id, x, advance, in points) as one `TJ` array, the pen
/// starting at `x`, with an adjustment wherever a glyph is not where the
/// previous one's advance ends (kerning, letter spacing). `em` is the
/// text space unit in points, and `round` rounds the adjustments.
pub(super) fn show_kerned(
    c: &mut Content,
    x: f32,
    em: f32,
    round: fn(f32) -> f32,
    glyphs: impl IntoIterator<Item = (u16, f32, f32)>,
) {
    let mut shown = c.show_positioned();
    let mut items = shown.items();
    let mut buf: Vec<u8> = Vec::new();
    let mut pen = x;
    for (id, x, advance) in glyphs {
        let delta = x - pen;
        if delta.abs() > KERN_EPS && em != 0.0 {
            if !buf.is_empty() {
                items.show(Str(&buf));
                buf.clear();
            }
            // TJ adjustments are in thousandths of text space, inverted.
            items.adjust(round(-delta * 1000.0 / em));
        }
        buf.extend_from_slice(&id.to_be_bytes());
        pen = x + advance;
    }
    if !buf.is_empty() {
        items.show(Str(&buf));
    }
}

/// Fills the current path with `rule`.
pub(super) fn fill(c: &mut Content, rule: FillRule) {
    match rule {
        FillRule::NonZero => c.fill_nonzero(),
        FillRule::EvenOdd => c.fill_even_odd(),
    };
}

/// Intersects the clip with the current path, with `rule`.
pub(super) fn clip(c: &mut Content, rule: FillRule) {
    match rule {
        FillRule::NonZero => c.clip_nonzero(),
        FillRule::EvenOdd => c.clip_even_odd(),
    };
}

pub(super) fn set_fill(c: &mut Content, color: &Color) {
    c.set_fill_rgb(round_pt(color[0]), round_pt(color[1]), round_pt(color[2]));
}

pub(super) fn path(c: &mut Content, segs: &[PathSeg]) {
    for s in segs {
        match *s {
            PathSeg::MoveTo(x, y) => {
                c.move_to(round_pt(x), round_pt(y));
            }
            PathSeg::LineTo(x, y) => {
                c.line_to(round_pt(x), round_pt(y));
            }
            PathSeg::CubicTo(x1, y1, x2, y2, x3, y3) => {
                c.cubic_to(
                    round_pt(x1),
                    round_pt(y1),
                    round_pt(x2),
                    round_pt(y2),
                    round_pt(x3),
                    round_pt(y3),
                );
            }
            PathSeg::Close => {
                c.close_path();
            }
        }
    }
}

/// Serialises one page's display list. Pure function of the page: safe to
/// run per page in parallel.
#[cfg(test)]
pub(crate) fn content_stream(
    page: &PageContent,
    fonts: &FontSet<'_>,
    profile: Profile,
) -> Result<Vec<u8>, InkoxideError> {
    marked_content_stream(page, fonts, profile, None).map(|(bytes, _)| bytes)
}

/// [`content_stream`], tagged with `plan`, with the owner of each MCID.
pub(crate) fn marked_content_stream(
    page: &PageContent,
    fonts: &FontSet<'_>,
    profile: Profile,
    plan: Option<&Plan>,
) -> Result<(Vec<u8>, Vec<StructId>), InkoxideError> {
    let mut c = Content::new();
    let mut marker = plan.map(|plan| Marker {
        plan,
        mcids: Vec::new(),
        tag: None,
        artifact: None,
        depth: 0,
    });
    // Flip to a top-left origin once; everything below is in layout space.
    c.transform([1.0, 0.0, 0.0, -1.0, 0.0, round_pt(page.height)]);
    for op in &page.ops {
        if let Some(m) = &mut marker {
            match op {
                Op::Tag(id) => m.tag(&mut c, *id),
                Op::EndTag => m.end_tag(&mut c),
                Op::Save => m.depth += 1,
                Op::Restore => {
                    m.restore(&mut c);
                    m.depth = m.depth.saturating_sub(1);
                }
                Op::FillRect { .. }
                | Op::FillPath { .. }
                | Op::StrokePath { .. }
                | Op::Text(_)
                | Op::Image { .. }
                | Op::Span { .. }
                | Op::InvisibleText(_) => m.draw(&mut c),
                _ => {}
            }
        }
        match op {
            Op::Save => {
                c.save_state();
            }
            Op::Restore => {
                c.restore_state();
            }
            Op::Transform(m) => {
                c.transform(m.map(|v| round_to(v, 1e5)));
            }
            Op::ClipRect(r) => {
                c.rect(round_pt(r.x), round_pt(r.y), round_pt(r.w), round_pt(r.h));
                c.clip_nonzero();
                c.end_path();
            }
            Op::ClipPath { path: p, rule } => {
                path(&mut c, &p.segs);
                clip(&mut c, *rule);
                c.end_path();
            }
            Op::FillRect { rect, color } => {
                set_fill(&mut c, color);
                c.rect(
                    round_pt(rect.x),
                    round_pt(rect.y),
                    round_pt(rect.w),
                    round_pt(rect.h),
                );
                c.fill_nonzero();
            }
            Op::FillPath {
                path: p,
                color,
                rule,
            } => {
                set_fill(&mut c, color);
                path(&mut c, &p.segs);
                fill(&mut c, *rule);
            }
            Op::StrokePath {
                path: p,
                color,
                width,
                dash,
            } => {
                c.save_state();
                c.set_stroke_rgb(round_pt(color[0]), round_pt(color[1]), round_pt(color[2]));
                c.set_line_width(round_pt(*width));
                if let Some(d) = dash {
                    c.set_dash_pattern(d.pattern.map(round_pt), 0.0);
                    if d.round_cap {
                        c.set_line_cap(LineCapStyle::RoundCap);
                    }
                }
                path(&mut c, &p.segs);
                c.stroke();
                c.restore_state();
            }
            Op::Link { .. } | Op::Field(_) | Op::Icon(_) | Op::Tag(_) | Op::EndTag => {}
            Op::SetOpacity(a) => {
                c.set_parameters(Name(gs_name(alpha_key(*a)).as_bytes()));
            }
            Op::Text(t) => show_text(&mut c, t, fonts, profile.is_pdf_a()),
            Op::Image { key, matrix } => {
                c.save_state();
                c.transform(matrix.map(round_pt));
                c.x_object(Name(image_name(*key).as_bytes()));
                c.restore_state();
            }
            Op::Span { actual_text } => {
                c.begin_marked_content_with_properties(Name(b"Span"))
                    .properties()
                    .pair(Name(b"ActualText"), Str(&out::text_bytes(actual_text)));
            }
            Op::EndSpan => {
                c.end_marked_content();
            }
            Op::InvisibleText(g) => {
                invisible_glyph(&mut c, g, font_name(g.font).as_bytes(), fonts, profile);
            }
        }
    }
    let mcids = match marker {
        Some(mut m) => {
            m.close_artifact(&mut c);
            m.end_tag(&mut c);
            m.mcids
        }
        None => Vec::new(),
    };
    Ok((content_bytes(c, profile)?, mcids))
}

/// An invisible glyph (`3 Tr`) in the font named `name`, horizontally
/// scaled (`Tz`) to its width, in its own save level: text state outlives
/// `ET`. Where `profile` bounds real numbers (PDF/A-1), a scaling past the
/// bound is moved into the font size, which draws the same width.
pub(super) fn invisible_glyph(
    c: &mut Content,
    g: &InvisibleGlyph,
    name: &[u8],
    fonts: &FontSet<'_>,
    profile: Profile,
) {
    let font = fonts.get(g.font);
    let advance = font.advance(g.glyph) * font.scale(g.size);
    let mut scaling = 100.0 * g.width / advance;
    if !(scaling.is_finite() && scaling > 0.0) {
        return;
    }
    let mut size = g.size;
    let max = profile.max_real();
    if scaling > max {
        size *= scaling / max;
        scaling = max;
    }
    c.save_state();
    c.begin_text();
    c.set_font(Name(name), round_pt(size));
    c.set_text_rendering_mode(TextRenderingMode::Invisible);
    c.set_horizontal_scaling(round_pt(scaling));
    c.set_text_matrix([1.0, 0.0, 0.0, -1.0, round_pt(g.x), round_pt(g.y)]);
    c.show(Str(&g.glyph.to_be_bytes()));
    c.end_text();
    c.restore_state();
}
