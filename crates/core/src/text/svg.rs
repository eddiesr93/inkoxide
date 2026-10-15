//! Lays out the text of SVG images with the render's fonts and shaper.
//!
//! Each `<text>` ([`SvgText`], read by `images/svg_text`) is split into
//! chunks at every absolute position. A chunk is shaped as one paragraph
//! ([`TextShaper::prepare`], so fallback fonts, bidi, the shape cache and
//! the glyph limit apply), then placed as `usvg` places text: spacing,
//! `textLength`, relative shifts and rotations per character, the anchor
//! per chunk and baselines from the font's metrics. Positions are in the
//! text's user space, y down.
//!
//! The glyphs of colour fonts stay glyphs: the PDF writer draws those
//! that have a colour image (`ColorGlyphs`, loaded after layout) as
//! images, as page text does. Emoji the document maps to images
//! (`emojiImages`) are laid out as the images ([`SvgImage`]).

use std::collections::HashMap;

use super::objects::InlineObject;
use super::{ParaStyle, TextShaper};
use crate::fonts::{FontKey, FontSet, LoadedFont};
use crate::images::svg_text::{Anchor, Baseline, Shift, SvgSpan, SvgText};
use crate::images::ImageKey;
use crate::paint::Rect;
use crate::schema::{Direction, FontStyle, TextAlign, TextRun, TextWrap, WhiteSpace};

/// A glyph of SVG text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SvgGlyph {
    pub font: FontKey,
    pub id: u16,
    /// Font size in user units.
    pub size: f32,
    /// The glyph origin on its baseline.
    pub x: f32,
    pub y: f32,
    /// Degrees, clockwise.
    pub rotate: f32,
    /// Horizontal stretch (`lengthAdjust="spacingAndGlyphs"`).
    pub scale_x: f32,
    /// The span style it is painted with ([`SvgText::paints`]).
    pub paint: u32,
    /// Its text, in [`LaidText::text`]: the text of its cluster, on the
    /// cluster's first glyph (an empty range: none).
    pub text: (u32, u32),
}

/// An underline, overline or line-through: a quadrilateral, filled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SvgDecoration {
    pub paint: u32,
    /// Corners in order around it.
    pub quad: [[f32; 2]; 4],
    /// A line-through, drawn over the glyphs (the others go under).
    pub over: bool,
}

/// An image in SVG text: an emoji the document maps to an image.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SvgImage {
    pub key: ImageKey,
    /// Its place on the baseline, as a glyph's.
    pub x: f32,
    pub y: f32,
    pub rotate: f32,
    pub scale_x: f32,
    /// The box it fills, from its place before rotation and stretch, y
    /// down.
    pub rect: Rect,
    pub placement: crate::images::Placement,
    /// What text extraction gets for it, through an invisible space of
    /// `font` at `size`, as wide as the image.
    pub text: Option<Box<str>>,
    pub font: FontKey,
    pub size: f32,
    pub paint: u32,
}

/// A text laid out.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LaidText {
    /// In paint style order, then in the order they are drawn.
    pub glyphs: Vec<SvgGlyph>,
    /// In paint style order, then in the order they are drawn.
    pub decorations: Vec<SvgDecoration>,
    /// In paint style order, then in the order they are drawn.
    pub images: Vec<SvgImage>,
    /// The box of the glyph outlines and decorations, `[x0, y0, x1, y1]`;
    /// `None` when nothing has ink.
    pub ink: Option<[f32; 4]>,
    /// The text of the glyphs ([`SvgGlyph::text`]).
    pub text: String,
}

impl LaidText {
    /// The text of glyph `g`, if it has any.
    pub fn glyph_text(&self, g: &SvgGlyph) -> Option<&str> {
        let (start, end) = (g.text.0 as usize, g.text.1 as usize);
        self.text.get(start..end).filter(|t| !t.is_empty())
    }

    /// The glyphs drawn in paint style `paint`.
    pub fn glyphs_of(&self, paint: u32) -> &[SvgGlyph] {
        let start = self.glyphs.partition_point(|g| g.paint < paint);
        let end = self.glyphs.partition_point(|g| g.paint <= paint);
        &self.glyphs[start..end]
    }

    /// The decorations drawn in paint style `paint`.
    pub fn decorations_of(&self, paint: u32) -> &[SvgDecoration] {
        let start = self.decorations.partition_point(|d| d.paint < paint);
        let end = self.decorations.partition_point(|d| d.paint <= paint);
        &self.decorations[start..end]
    }

    /// A layout without fonts: a glyph of each of `paints` paint styles
    /// on the unit square, for what a check needs of the text (its paint).
    pub fn unit(paints: usize) -> Self {
        let glyph = |paint| SvgGlyph {
            font: 0,
            id: 0,
            size: 1.0,
            x: 0.0,
            y: 0.0,
            rotate: 0.0,
            scale_x: 1.0,
            paint,
            text: (0, 0),
        };
        Self {
            glyphs: (0..paints as u32).map(glyph).collect(),
            decorations: Vec::new(),
            images: Vec::new(),
            ink: Some([0.0, 0.0, 1.0, 1.0]),
            text: String::new(),
        }
    }

    /// The images drawn in paint style `paint`.
    pub fn images_of(&self, paint: u32) -> &[SvgImage] {
        let start = self.images.partition_point(|i| i.paint < paint);
        let end = self.images.partition_point(|i| i.paint <= paint);
        &self.images[start..end]
    }
}

/// A glyph of a cluster, as shaped.
#[derive(Debug, Clone)]
struct ShapedGlyph {
    id: u16,
    /// The text of its cluster, on the cluster's first glyph.
    text: Option<Box<str>>,
    /// Offsets from the pen, in user units, y up.
    x_offset: f32,
    y_offset: f32,
    advance: f32,
}

/// A cluster: glyphs drawn for one or more characters, in visual order.
#[derive(Debug, Clone)]
struct Cluster {
    /// Index of its first character in [`SvgText::chars`].
    char: usize,
    span: usize,
    font: FontKey,
    glyphs: Vec<ShapedGlyph>,
    advance: f32,
    scale_x: f32,
    space: bool,
    /// An image in place of its one glyph, and how far it reaches above
    /// the baseline.
    object: Option<(InlineObject, f32)>,
}

/// A chunk: clusters placed from one absolute position.
struct Chunk {
    x: Option<f32>,
    y: Option<f32>,
    /// The anchor of its first character.
    anchor: Anchor,
    clusters: Vec<Cluster>,
    rtl: bool,
}

/// Lays out `text` with `shaper`'s fonts.
pub(crate) fn lay_out<S: TextShaper>(text: &SvgText, shaper: &mut S) -> LaidText {
    let keys: Vec<Option<FontKey>> = {
        let fonts = shaper.fonts();
        text.spans.iter().map(|s| font_of(s, fonts)).collect()
    };
    let drawn = |i: usize| {
        let c = &text.chars[i];
        let span = c.span as usize;
        !c.hidden && keys[span].is_some() && valid_size(text.spans[span].size)
    };
    // Chunks start at characters with an absolute position.
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for i in (0..text.chars.len()).filter(|&i| drawn(i)) {
        let c = &text.chars[i];
        match ranges.last_mut() {
            Some(r) if c.x.is_none() && c.y.is_none() => r.end = i + 1,
            _ => ranges.push(i..i + 1),
        }
    }
    let mut chunks: Vec<Chunk> = ranges
        .into_iter()
        .map(|r| {
            let first = text.chars[r.start];
            let chars: Vec<usize> = r.filter(|&i| drawn(i)).collect();
            Chunk {
                x: first.x,
                y: first.y,
                anchor: text.spans[first.span as usize].anchor,
                clusters: shape(text, &chars, &keys, shaper),
                rtl: text.rtl,
            }
        })
        .collect();
    let fonts = shaper.fonts();
    for chunk in &mut chunks {
        spacing(text, &mut chunk.clusters);
    }
    length_adjust(text, &mut chunks);
    place(text, &chunks, fonts)
}

/// The declared font CSS matching picks for `span`.
fn font_of(span: &SvgSpan, fonts: &FontSet<'_>) -> Option<FontKey> {
    let families: Vec<&str> = span.families.iter().map(String::as_str).collect();
    let style = if span.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    fonts.select(&families, span.weight, style)
}

fn valid_size(size: f32) -> bool {
    size.is_finite() && size > 0.0
}

/// Shapes characters `chars` (one chunk, in logical order) and returns
/// their clusters in visual order.
fn shape<S: TextShaper>(
    text: &SvgText,
    chars: &[usize],
    keys: &[Option<FontKey>],
    shaper: &mut S,
) -> Vec<Cluster> {
    // One run per stretch of characters of one span.
    let mut runs: Vec<TextRun> = Vec::new();
    let mut run_span: Vec<usize> = Vec::new();
    // Byte offset of each character in the chunk's text, and its index.
    let mut offsets: Vec<(u32, usize)> = Vec::with_capacity(chars.len());
    let mut bytes = 0u32;
    for &i in chars {
        let c = text.chars[i];
        let span = c.span as usize;
        if run_span.last() != Some(&span) {
            let s = &text.spans[span];
            let Some(key) = keys[span] else { continue };
            runs.push(TextRun {
                text: Some(String::new()),
                font: shaper.fonts().get(key).id.to_string(),
                size: s.size,
                color: None,
                letter_spacing: (s.letter_spacing != 0.0).then_some(s.letter_spacing),
                decoration: None,
                decoration_color: None,
                decoration_thickness: None,
                decoration_style: None,
                placeholder: None,
                link: None,
                hyphenate: Some(false),
                // SVG text shifts baselines itself (`baseline-shift`).
                vertical_align: None,
                font_features: None,
                image: None,
                note: None,
            });
            run_span.push(span);
        }
        if let Some(t) = runs.last_mut().and_then(|r| r.text.as_mut()) {
            t.push(c.c);
        }
        offsets.push((bytes, i));
        bytes += c.c.len_utf8() as u32;
    }
    let style = ParaStyle {
        align: TextAlign::Start,
        line_height: None,
        max_lines: None,
        ellipsis: false,
        white_space: WhiteSpace::Pre,
        // One line: nothing to break.
        text_wrap: TextWrap::Wrap,
        indent: 0.0,
        direction: if text.rtl {
            Direction::Rtl
        } else {
            Direction::Ltr
        },
        hyphenate: false,
        lang: None,
    };
    let prepared = shaper.prepare(&runs, &style);
    // Glyphs in visual order, each with its bidi level.
    let mut order: Vec<(usize, u8)> = Vec::with_capacity(prepared.glyphs.len());
    for seg in &prepared.segments {
        order.extend((seg.glyphs.start as usize..seg.glyphs.end as usize).map(|g| (g, seg.level)));
    }
    super::paint::reorder(&mut order, |g| g.1);
    let char_at = |cluster: u32| {
        let k = offsets
            .partition_point(|o| o.0 <= cluster)
            .saturating_sub(1);
        offsets.get(k).map_or(0, |o| o.1)
    };
    let mut clusters: Vec<Cluster> = Vec::new();
    for (g, _) in order {
        let glyph = prepared.glyphs[g];
        let Some(seg) = prepared.segment_of(g) else {
            continue;
        };
        let char = char_at(glyph.cluster);
        let span = text.chars[char].span as usize;
        let item = ShapedGlyph {
            id: glyph.id,
            text: prepared.glyph_text(seg, g).map(Box::from),
            x_offset: glyph.x_offset,
            y_offset: glyph.y_offset,
            advance: glyph.advance,
        };
        let object = seg.object.and_then(|o| prepared.objects.get(o as usize));
        let object = object.map(|o| {
            let run = &runs[usize::from(seg.run)];
            let above = o.extent(run, shaper.fonts()).map_or(o.height, |e| e.0);
            (o.clone(), above)
        });
        match clusters.last_mut() {
            Some(c)
                if c.char == char
                    && c.font == seg.font
                    && object.is_none()
                    && c.object.is_none() =>
            {
                c.glyphs.push(item);
                c.advance += glyph.advance;
            }
            _ => clusters.push(Cluster {
                char,
                span,
                font: seg.font,
                glyphs: vec![item],
                advance: glyph.advance,
                scale_x: 1.0,
                space: glyph.is_space(),
                object,
            }),
        }
    }
    clusters
}

/// Word spacing after spaces; no letter spacing after the last cluster
/// (`apply_letter_spacing`, `apply_word_spacing`).
fn spacing(text: &SvgText, clusters: &mut [Cluster]) {
    for c in clusters.iter_mut() {
        let span = &text.spans[c.span];
        if c.space && span.word_spacing != 0.0 {
            c.advance += span.word_spacing;
        }
    }
    if let Some(last) = clusters.last_mut() {
        last.advance -= text.spans[last.span].letter_spacing;
    }
}

/// `textLength`: spreads the difference over the gaps between clusters
/// (`spacing`), or stretches them (`spacingAndGlyphs`). Innermost first.
/// Each finds its clusters by character in an index, so the work is the
/// clusters inside each, not all of them.
fn length_adjust(text: &SvgText, chunks: &mut [Chunk]) {
    if text.adjust.is_empty() {
        return;
    }
    // Every cluster as (chunk, index), in the order they are drawn, and
    // their positions in that order by first character.
    let all: Vec<(usize, usize)> = chunks
        .iter()
        .enumerate()
        .flat_map(|(k, c)| (0..c.clusters.len()).map(move |i| (k, i)))
        .collect();
    let mut by_char: Vec<(usize, usize)> = all
        .iter()
        .enumerate()
        .map(|(at, &(k, i))| (chunks[k].clusters[i].char, at))
        .collect();
    by_char.sort_unstable();
    let mut order: Vec<usize> = Vec::new();
    for adjust in &text.adjust {
        let start = by_char.partition_point(|e| e.0 < adjust.chars.start);
        let end = by_char.partition_point(|e| e.0 < adjust.chars.end);
        // In the order they are drawn, which the sums and the last gap
        // follow.
        order.clear();
        order.extend(by_char[start..end].iter().map(|e| e.1));
        order.sort_unstable();
        let inside = order.iter().map(|&at| all[at]);
        let n = order.len();
        if n == 0 {
            continue;
        }
        let width: f32 = inside
            .clone()
            .map(|(k, i)| chunks[k].clusters[i].advance)
            .sum();
        if adjust.glyphs {
            let factor = adjust.length / width;
            // Nothing to stretch, or nearly nothing left.
            if !factor.is_finite() || factor < 0.001 {
                continue;
            }
            for (k, i) in inside {
                let c = &mut chunks[k].clusters[i];
                c.scale_x *= factor;
                c.advance *= factor;
            }
        } else if n > 1 {
            let extra = (adjust.length - width) / (n - 1) as f32;
            for (k, i) in inside.take(n - 1) {
                chunks[k].clusters[i].advance += extra;
            }
        }
    }
}

/// How far the baseline of `span` sits below the text position, in user
/// units (`resolve_baseline`).
fn baseline_shift(span: &SvgSpan, font: &LoadedFont<'_>) -> f32 {
    let m = &font.data.metrics;
    let scale = font.scale(span.size);
    let mut up = 0.0;
    for s in span.shift.iter().rev() {
        up += match *s {
            Shift::Sub => -m.subscript_offset * scale,
            Shift::Super => m.superscript_offset * scale,
            Shift::Length(n) => n,
        };
    }
    let (ascent, descent) = (m.ascender * scale, m.descender * scale);
    let down = match span.baseline {
        Baseline::Alphabetic => 0.0,
        Baseline::TextBeforeEdge => ascent,
        Baseline::Middle => m.x_height * scale * 0.5,
        Baseline::Central => ascent - (ascent - descent) * 0.5,
        Baseline::TextAfterEdge | Baseline::Ideographic => descent,
        Baseline::Hanging => ascent * 0.8,
        Baseline::Mathematical => ascent * 0.5,
    };
    down - up
}

/// A point turned by `deg` degrees (clockwise, y down) about the origin.
fn turn((x, y): (f32, f32), deg: f32) -> (f32, f32) {
    if deg == 0.0 {
        return (x, y);
    }
    let (sin, cos) = deg.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// The box of `points`, grown into `ink`.
fn grow(ink: &mut Option<[f32; 4]>, points: impl IntoIterator<Item = (f32, f32)>) {
    for (x, y) in points {
        if !(x.is_finite() && y.is_finite()) {
            continue;
        }
        let b = ink.get_or_insert([x, y, x, y]);
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
}

/// Places the clusters of every chunk (`resolve_clusters_positions`),
/// with anchors, baselines, decorations and the ink box.
fn place(text: &SvgText, chunks: &[Chunk], fonts: &FontSet<'_>) -> LaidText {
    let mut out = LaidText::default();
    // Outline boxes by font and glyph: each outline is read once.
    let mut boxes: HashMap<(FontKey, u16), Option<[f32; 4]>> = HashMap::new();
    let (mut last_x, mut last_y) = (0.0f32, 0.0f32);
    for chunk in chunks {
        if chunk.clusters.is_empty() {
            continue;
        }
        let (x0, y0) = (chunk.x.unwrap_or(last_x), chunk.y.unwrap_or(last_y));
        let width: f32 = chunk.clusters.iter().map(|c| c.advance).sum();
        // Start and end swap in right-to-left text.
        let mut x = match (chunk.anchor, chunk.rtl) {
            (Anchor::Start, false) | (Anchor::End, true) => 0.0,
            (Anchor::Middle, _) => -width / 2.0,
            (Anchor::End, false) | (Anchor::Start, true) => -width,
        };
        let mut y = 0.0;
        let mut runs: Vec<DecorationRun> = Vec::new();
        for c in &chunk.clusters {
            let ch = text.chars[c.char];
            x += ch.dx;
            y += ch.dy;
            let shifted = ch.dx != 0.0 || ch.dy != 0.0 || ch.rotate != 0.0;
            let span = &text.spans[c.span];
            let font = fonts.get(c.font);
            let down = baseline_shift(span, font);
            let (ox, oy) = (x0 + x, y0 + y + down);
            let scale = font.scale(span.size);
            let mut pen = 0.0;
            if let Some((o, above)) = &c.object {
                let image = SvgImage {
                    key: o.key,
                    x: ox,
                    y: oy,
                    rotate: ch.rotate,
                    scale_x: c.scale_x,
                    rect: Rect::new(0.0, -above, o.width, o.height),
                    placement: o.placement,
                    text: o.alt.clone(),
                    font: c.font,
                    size: span.size,
                    paint: span.paint,
                };
                let r = image.rect;
                let corners = [
                    (r.x, r.y),
                    (r.x + r.w, r.y),
                    (r.x + r.w, r.y + r.h),
                    (r.x, r.y + r.h),
                ];
                grow(
                    &mut out.ink,
                    corners.map(|(u, v)| {
                        let (px, py) = turn((u * c.scale_x, v), ch.rotate);
                        (ox + px, oy + py)
                    }),
                );
                out.images.push(image);
            }
            let glyphs = if c.object.is_some() {
                &[][..]
            } else {
                &c.glyphs[..]
            };
            for ShapedGlyph {
                id,
                text: glyph_text,
                x_offset,
                y_offset,
                advance,
            } in glyphs
            {
                let (id, x_offset, y_offset, advance) = (*id, *x_offset, *y_offset, *advance);
                let start = out.text.len() as u32;
                if let Some(t) = glyph_text {
                    out.text.push_str(t);
                }
                let (gx, gy) = turn(((pen + x_offset) * c.scale_x, -y_offset), ch.rotate);
                let g = SvgGlyph {
                    font: c.font,
                    id,
                    size: span.size,
                    x: ox + gx,
                    y: oy + gy,
                    rotate: ch.rotate,
                    scale_x: c.scale_x,
                    paint: span.paint,
                    text: (start, out.text.len() as u32),
                };
                let bbox = *boxes
                    .entry((c.font, id))
                    .or_insert_with(|| ink_box(font, id));
                if let Some([bx0, by0, bx1, by1]) = bbox {
                    let corners = [(bx0, by0), (bx1, by0), (bx1, by1), (bx0, by1)];
                    grow(
                        &mut out.ink,
                        corners.map(|(u, v)| {
                            let (px, py) = turn((u * scale * c.scale_x, -v * scale), g.rotate);
                            (g.x + px, g.y + py)
                        }),
                    );
                }
                out.glyphs.push(g);
                pen += advance;
            }
            match runs.last_mut() {
                Some(r) if r.span == c.span && !shifted => r.width += c.advance,
                _ => runs.push(DecorationRun {
                    span: c.span,
                    font: c.font,
                    origin: (ox, oy),
                    rotate: ch.rotate,
                    width: c.advance,
                }),
            }
            x += c.advance;
        }
        for run in runs {
            let span = &text.spans[run.span];
            let font = fonts.get(run.font);
            let (rx, ry) = run.origin;
            let (rotate, w) = (run.rotate, run.width);
            let m = &font.data.metrics;
            let scale = font.scale(span.size);
            let thick = m.underline_thickness * scale;
            for (on, offset, over) in [
                (span.underline, -m.underline_position * scale, false),
                (span.overline, -m.ascender * scale, false),
                (span.line_through, -m.strikeout_position * scale, true),
            ] {
                if !(on && thick > 0.0 && w > 0.0) {
                    continue;
                }
                let (top, bottom) = (offset - thick / 2.0, offset + thick / 2.0);
                let quad = [(0.0, top), (w, top), (w, bottom), (0.0, bottom)].map(|p| {
                    let (px, py) = turn(p, rotate);
                    [rx + px, ry + py]
                });
                grow(&mut out.ink, quad.map(|p| (p[0], p[1])));
                out.decorations.push(SvgDecoration {
                    paint: span.paint,
                    quad,
                    over,
                });
            }
        }
        last_x = x0 + x;
        last_y = y0 + y;
    }
    // Each probe draws one paint style: stable sorts keep the order.
    out.glyphs.sort_by_key(|g| g.paint);
    out.decorations.sort_by_key(|d| d.paint);
    out.images.sort_by_key(|i| i.paint);
    out
}

/// The box of glyph `id`'s ink, in font units, y up: its outline, or the
/// em box of a colour glyph without one.
fn ink_box(font: &LoadedFont<'_>, id: u16) -> Option<[f32; 4]> {
    let m = &font.data.metrics;
    font.glyph_bbox(id).or_else(|| {
        m.color
            .then(|| [0.0, m.descender, font.advance(id), m.ascender])
    })
}

/// Clusters of one span without a shift between them, which a decoration
/// spans (as `collect_decoration_spans` groups them).
struct DecorationRun {
    span: usize,
    /// The font of its first cluster, whose metrics place the lines.
    font: FontKey,
    /// Where it starts, on the baseline.
    origin: (f32, f32),
    rotate: f32,
    width: f32,
}

#[cfg(test)]
mod tests;
