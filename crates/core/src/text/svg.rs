//! Lays out the text of SVG images with the render's fonts and shaper.
//!
//! Each `<text>` ([`SvgText`], read by `images/svg_text`) is split into
//! chunks at every absolute position. A chunk is shaped as one paragraph
//! ([`TextShaper::prepare`], so fallback fonts, bidi, the shape cache and
//! the glyph limit apply), then placed as `usvg` places text: spacing,
//! `textLength`, relative shifts and rotations per character, the anchor
//! per chunk and baselines from the font's metrics. Positions are in the
//! text's user space, y down.

use super::{ParaStyle, TextShaper};
use crate::fonts::{FontKey, FontSet, LoadedFont};
use crate::images::svg_text::{Anchor, Baseline, Shift, SvgSpan, SvgText};
use crate::schema::{Direction, FontStyle, TextAlign, TextRun, WhiteSpace};

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

/// A text laid out.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LaidText {
    pub glyphs: Vec<SvgGlyph>,
    pub decorations: Vec<SvgDecoration>,
    /// The box of the glyph outlines and decorations, `[x0, y0, x1, y1]`;
    /// `None` when nothing has ink.
    pub ink: Option<[f32; 4]>,
}

/// A cluster: glyphs drawn for one or more characters, in visual order.
#[derive(Debug, Clone)]
struct Cluster {
    /// Index of its first character in [`SvgText::chars`].
    char: usize,
    span: usize,
    font: FontKey,
    /// Glyph id, offset (points, y up) and advance.
    glyphs: Vec<(u16, f32, f32, f32)>,
    advance: f32,
    scale_x: f32,
    space: bool,
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
    fonts.select(&families, span.weight, style).map(|m| m.key)
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
        let item = (glyph.id, glyph.x_offset, glyph.y_offset, glyph.advance);
        match clusters.last_mut() {
            Some(c) if c.char == char && c.font == seg.font => {
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
fn length_adjust(text: &SvgText, chunks: &mut [Chunk]) {
    for adjust in &text.adjust {
        let mut inside: Vec<&mut Cluster> = chunks
            .iter_mut()
            .flat_map(|c| c.clusters.iter_mut())
            .filter(|c| adjust.chars.contains(&c.char))
            .collect();
        if inside.is_empty() {
            continue;
        }
        let width: f32 = inside.iter().map(|c| c.advance).sum();
        if adjust.glyphs {
            let factor = adjust.length / width;
            // Nothing to stretch, or nearly nothing left.
            if !factor.is_finite() || factor < 0.001 {
                continue;
            }
            for c in &mut inside {
                c.scale_x *= factor;
                c.advance *= factor;
            }
        } else if inside.len() > 1 {
            let extra = (adjust.length - width) / (inside.len() - 1) as f32;
            let n = inside.len();
            for c in inside.iter_mut().take(n - 1) {
                c.advance += extra;
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
        // Decoration runs: clusters of one span without a shift between
        // them, as `collect_decoration_spans` groups them.
        let mut runs: Vec<(usize, f32, f32, f32, f32)> = Vec::new();
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
            for &(id, xo, yo, adv) in &c.glyphs {
                let (gx, gy) = turn(((pen + xo) * c.scale_x, -yo), ch.rotate);
                let g = SvgGlyph {
                    font: c.font,
                    id,
                    size: span.size,
                    x: ox + gx,
                    y: oy + gy,
                    rotate: ch.rotate,
                    scale_x: c.scale_x,
                    paint: span.paint,
                };
                if let Some([bx0, by0, bx1, by1]) = font.glyph_bbox(id) {
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
                pen += adv;
            }
            match runs.last_mut() {
                Some(r) if r.0 == c.span && !shifted => r.4 += c.advance,
                _ => runs.push((c.span, ox, oy, ch.rotate, c.advance)),
            }
            x += c.advance;
        }
        for (span_index, rx, ry, rotate, w) in runs {
            let span = &text.spans[span_index];
            let font = chunk
                .clusters
                .iter()
                .find(|c| c.span == span_index)
                .map(|c| fonts.get(c.font));
            let Some(font) = font else { continue };
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
    out
}

#[cfg(test)]
mod tests;
