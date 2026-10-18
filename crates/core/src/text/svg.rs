//! Lays out the text of SVG images with the render's fonts and shaper.
//!
//! Each `<text>` ([`SvgText`], read by `images/svg_text`) is split into
//! chunks at every absolute position, and where a `textPath` starts or
//! ends. A chunk is shaped as one paragraph
//! ([`TextShaper::prepare`], so fallback fonts, bidi, the shape cache and
//! the glyph limit apply), then placed as `usvg` places text: spacing,
//! `textLength`, relative shifts and rotations per character, the anchor
//! per chunk and baselines from the font's metrics. Positions are in the
//! text's user space, y down.
//!
//! Vertical text (`writing-mode`) is laid out down a column: runs of
//! upright characters shaped top to bottom, with the font's vertical
//! metrics and forms, and runs of sideways ones shaped as horizontal text
//! turned a quarter, all centred on the column.
//!
//! The glyphs of colour fonts stay glyphs: the PDF writer draws those
//! that have a colour image (`ColorGlyphs`, loaded after layout) as
//! images, as page text does. Emoji the document maps to images
//! (`emojiImages`) are laid out as the images ([`SvgImage`]).

use std::collections::HashMap;

use super::objects::InlineObject;
use super::vertical;
use super::{ParaStyle, TextShaper};
use crate::fonts::{Ask, FontKey, FontSet, LoadedFont};
use crate::geom::{self, Rect};
use crate::images::svg_text::{
    Anchor, Baseline, Shift, SvgSpan, SvgText, TextOrientation, TextPathSpec, Writing,
};
use crate::images::ImageKey;
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
    /// Its cluster's first character, in [`SvgText::chars`].
    pub char: u32,
    /// Standing upright in vertical text.
    pub upright: bool,
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
    pub placement: crate::images::ImagePlacement,
    /// What text extraction gets for it, through an invisible space of
    /// `font` at `size`, as wide as the image.
    pub text: Option<Box<str>>,
    pub font: FontKey,
    pub size: f32,
    pub paint: u32,
    /// Its character, in [`SvgText::chars`].
    pub char: u32,
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
    /// For text on a path or vertical text, by paint style: the text it
    /// draws in logical order, which text extraction reads in place of
    /// the glyphs (`ActualText`), placed as they are; empty otherwise.
    pub actual: Vec<String>,
}

/// The items of `items` (in paint style order) drawn in paint style
/// `paint`.
fn of_paint<T>(items: &[T], paint: u32, paint_of: impl Fn(&T) -> u32) -> &[T] {
    let start = items.partition_point(|i| paint_of(i) < paint);
    let end = items.partition_point(|i| paint_of(i) <= paint);
    &items[start..end]
}

impl LaidText {
    /// The text of glyph `g`, if it has any.
    pub fn glyph_text(&self, g: &SvgGlyph) -> Option<&str> {
        let (start, end) = (g.text.0 as usize, g.text.1 as usize);
        self.text.get(start..end).filter(|t| !t.is_empty())
    }

    /// The glyphs drawn in paint style `paint`.
    pub fn glyphs_of(&self, paint: u32) -> &[SvgGlyph] {
        of_paint(&self.glyphs, paint, |g| g.paint)
    }

    /// The decorations drawn in paint style `paint`.
    pub fn decorations_of(&self, paint: u32) -> &[SvgDecoration] {
        of_paint(&self.decorations, paint, |d| d.paint)
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
            char: 0,
            upright: false,
        };
        Self {
            glyphs: (0..paints as u32).map(glyph).collect(),
            decorations: Vec::new(),
            images: Vec::new(),
            ink: Some([0.0, 0.0, 1.0, 1.0]),
            text: String::new(),
            actual: Vec::new(),
        }
    }

    /// The images drawn in paint style `paint`.
    pub fn images_of(&self, paint: u32) -> &[SvgImage] {
        of_paint(&self.images, paint, |i| i.paint)
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
    /// Upright in vertical text: shaped top to bottom, its advance down.
    upright: bool,
}

/// A chunk: clusters placed from one absolute position.
struct Chunk {
    x: Option<f32>,
    y: Option<f32>,
    /// The path it goes along ([`SvgText::paths`]).
    path: Option<u16>,
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
    // Chunks start at characters with an absolute position, and where
    // the path changes.
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for i in (0..text.chars.len()).filter(|&i| drawn(i)) {
        let c = &text.chars[i];
        match ranges.last_mut() {
            Some(r) if c.x.is_none() && c.y.is_none() && text.chars[r.end - 1].path == c.path => {
                r.end = i + 1;
            }
            _ => ranges.push(i..i + 1),
        }
    }
    let mut forms = HashMap::new();
    let mut chunks: Vec<Chunk> = ranges
        .into_iter()
        .map(|r| {
            let first = text.chars[r.start];
            let chars: Vec<usize> = r.filter(|&i| drawn(i)).collect();
            let clusters = if text.writing.is_horizontal() {
                shape(text, &chars, &keys, shaper, false)
            } else {
                shape_column(text, &chars, &keys, shaper, &mut forms)
            };
            Chunk {
                x: first.x,
                y: first.y,
                path: first.path,
                anchor: text.spans[first.span as usize].anchor,
                clusters,
                rtl: text.rtl,
            }
        })
        .collect();
    let fonts = shaper.fonts();
    for chunk in &mut chunks {
        spacing(text, &mut chunk.clusters);
    }
    length_adjust(text, &mut chunks);
    let mut laid = if text.writing.is_horizontal() {
        place(text, &chunks, fonts)
    } else {
        place_vertical(text, &chunks, fonts)
    };
    if !text.writing.is_horizontal() || !text.paths.is_empty() {
        laid.actual = actual_text(&laid, text.paints.len());
    }
    laid
}

/// The text each paint style of `laid` draws, in logical order: the text
/// of its glyphs and the alternative text of its images, by character.
fn actual_text(laid: &LaidText, paints: usize) -> Vec<String> {
    let mut pieces: Vec<(u32, u32, &str)> = laid
        .glyphs
        .iter()
        .filter_map(|g| Some((g.paint, g.char, laid.glyph_text(g)?)))
        .chain(
            laid.images
                .iter()
                .filter_map(|i| Some((i.paint, i.char, i.text.as_deref()?))),
        )
        .collect();
    pieces.sort_by_key(|p| (p.0, p.1));
    let mut out = vec![String::new(); paints];
    for (paint, _, t) in pieces {
        if let Some(s) = out.get_mut(paint as usize) {
            s.push_str(t);
        }
    }
    out
}

/// Shapes characters `chars` of vertical text (one chunk, in logical
/// order): each run of upright characters top to bottom, each run of
/// sideways ones as horizontal text. `forms` keeps which characters have
/// a vertical form in which font.
fn shape_column<S: TextShaper>(
    text: &SvgText,
    chars: &[usize],
    keys: &[Option<FontKey>],
    shaper: &mut S,
    forms: &mut HashMap<(FontKey, char), bool>,
) -> Vec<Cluster> {
    let upright: Vec<bool> = chars
        .iter()
        .map(|&i| stands_upright(text, i, keys, shaper.fonts(), forms))
        .collect();
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let up = upright[start];
        let end = start + upright[start..].iter().take_while(|&&u| u == up).count();
        out.extend(shape(text, &chars[start..end], keys, shaper, up));
        start = end;
    }
    out
}

/// Whether character `i` of vertical text stands upright: by its span's
/// `text-orientation`, and when that is `mixed`, by its Unicode
/// `Vertical_Orientation` (`Tr`: upright when its font has a vertical
/// form for it).
fn stands_upright(
    text: &SvgText,
    i: usize,
    keys: &[Option<FontKey>],
    fonts: &FontSet<'_>,
    forms: &mut HashMap<(FontKey, char), bool>,
) -> bool {
    if text.writing != Writing::Vertical {
        return false;
    }
    let c = text.chars[i];
    let span = c.span as usize;
    match text.spans[span].orientation {
        TextOrientation::Upright => true,
        TextOrientation::Sideways => false,
        TextOrientation::Mixed => match vertical::orientation(c.c) {
            None => false,
            Some(vertical::Orientation::Upright | vertical::Orientation::TransformedUpright) => {
                true
            }
            Some(vertical::Orientation::TransformedRotated) => keys[span].is_some_and(|k| {
                *forms
                    .entry((k, c.c))
                    .or_insert_with(|| fonts.get(k).has_vertical_form(c.c))
            }),
        },
    }
}

/// The declared font CSS matching picks for `span`.
fn font_of(span: &SvgSpan, fonts: &FontSet<'_>) -> Option<FontKey> {
    let families: Vec<&str> = span.families.iter().map(String::as_str).collect();
    let style = if span.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let key = fonts.select(&families, f32::from(span.weight), style, span.stretch)?;
    Some(fonts.variant(key, ask(span)))
}

/// What `span` asks of its font: its size, weight and width.
pub(crate) fn ask(span: &SvgSpan) -> Ask {
    Ask {
        size: span.size,
        weight: Some(f32::from(span.weight)),
        stretch: Some(span.stretch),
    }
}

fn valid_size(size: f32) -> bool {
    size.is_finite() && size > 0.0
}

/// Shapes characters `chars` (one chunk, or a run of one, in logical
/// order) and returns their clusters in visual order; `upright`: top to
/// bottom.
fn shape<S: TextShaper>(
    text: &SvgText,
    chars: &[usize],
    keys: &[Option<FontKey>],
    shaper: &mut S,
    upright: bool,
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
                font_key: Some(key),
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
                font_features: (!s.features.is_empty()).then(|| s.features.to_vec().into()),
                image: None,
                note: None,
                attachment: None,
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
        hyphenation_penalty: super::HYPHENATION_PENALTY,
        indent: 0.0,
        direction: if text.rtl {
            Direction::Rtl
        } else {
            Direction::Ltr
        },
        hyphenate: false,
        lang: None,
        vertical: upright,
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
            let run = &runs[seg.run as usize];
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
                upright,
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
fn baseline_drop(span: &SvgSpan, font: &LoadedFont<'_>) -> f32 {
    let m = font.metrics();
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

/// How far the alphabetic baseline of `span` sits below its central one:
/// the middle of the font's ascender and descender.
fn central_drop(span: &SvgSpan, font: &LoadedFont<'_>) -> f32 {
    let m = font.metrics();
    (m.ascender + m.descender) * font.scale(span.size) / 2.0
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

/// Outline boxes by font and glyph: each outline is read once.
type Boxes = HashMap<(FontKey, u16), Option<[f32; 4]>>;

/// Draws cluster `c` (of span `span`, in `font`) from `origin` turned
/// `rotate` degrees: its glyphs, or its image, into `out`, with their
/// ink. A horizontal cluster's origin is on its baseline; an upright
/// one's, on the centre of the column at its top.
fn put_cluster(
    out: &mut LaidText,
    boxes: &mut Boxes,
    (c, span, font): (&Cluster, &SvgSpan, &LoadedFont<'_>),
    (ox, oy): (f32, f32),
    rotate: f32,
) {
    let scale = font.scale(span.size);
    // Upright glyphs are not stretched: their advance is.
    let scale_x = if c.upright { 1.0 } else { c.scale_x };
    if let Some((o, above)) = &c.object {
        let rect = if c.upright {
            Rect::new(-o.width / 2.0, 0.0, o.width, o.height)
        } else {
            Rect::new(0.0, -above, o.width, o.height)
        };
        let image = SvgImage {
            key: o.key,
            x: ox,
            y: oy,
            rotate,
            scale_x,
            rect,
            placement: o.placement,
            text: o.alt.clone(),
            font: c.font,
            size: span.size,
            paint: span.paint,
            char: c.char as u32,
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
                let (px, py) = geom::rotate((u * scale_x, v), rotate);
                (ox + px, oy + py)
            }),
        );
        out.images.push(image);
        return;
    }
    let mut pen = 0.0;
    for ShapedGlyph {
        id,
        text: glyph_text,
        x_offset,
        y_offset,
        advance,
    } in &c.glyphs
    {
        let (id, x_offset, y_offset, advance) = (*id, *x_offset, *y_offset, *advance);
        let start = out.text.len() as u32;
        if let Some(t) = glyph_text {
            out.text.push_str(t);
        }
        // Upright glyphs go down from the column's top, sideways and
        // horizontal ones along their baseline.
        let at = if c.upright {
            (x_offset, pen - y_offset)
        } else {
            ((pen + x_offset) * scale_x, -y_offset)
        };
        let (gx, gy) = geom::rotate(at, rotate);
        let g = SvgGlyph {
            font: c.font,
            id,
            size: span.size,
            x: ox + gx,
            y: oy + gy,
            rotate,
            scale_x,
            paint: span.paint,
            text: (start, out.text.len() as u32),
            char: c.char as u32,
            upright: c.upright,
        };
        let bbox = *boxes
            .entry((c.font, id))
            .or_insert_with(|| ink_box(font, id));
        if let Some([bx0, by0, bx1, by1]) = bbox {
            let corners = [(bx0, by0), (bx1, by0), (bx1, by1), (bx0, by1)];
            grow(
                &mut out.ink,
                corners.map(|(u, v)| {
                    let (px, py) = geom::rotate((u * scale * scale_x, -v * scale), g.rotate);
                    (g.x + px, g.y + py)
                }),
            );
        }
        out.glyphs.push(g);
        pen += advance;
    }
}

/// Where the clusters of a chunk start along its line, from its anchor:
/// start and end swap in right-to-left text.
fn anchored(chunk: &Chunk) -> f32 {
    let width: f32 = chunk.clusters.iter().map(|c| c.advance).sum();
    match (chunk.anchor, chunk.rtl) {
        (Anchor::Start, false) | (Anchor::End, true) => 0.0,
        (Anchor::Middle, _) => -width / 2.0,
        (Anchor::End, false) | (Anchor::Start, true) => -width,
    }
}

/// What placing the chunks of a text builds: the laid text, the outline
/// boxes read so far, and the decoration runs of the chunk being placed.
#[derive(Default)]
struct Placing {
    out: LaidText,
    boxes: Boxes,
    runs: Vec<DecorationRun>,
}

impl Placing {
    /// Draws a cluster from `origin` turned `rotate` ([`put_cluster`]).
    fn put(
        &mut self,
        cluster: (&Cluster, &SvgSpan, &LoadedFont<'_>),
        origin: (f32, f32),
        rotate: f32,
    ) {
        put_cluster(&mut self.out, &mut self.boxes, cluster, origin, rotate);
    }
}

/// Places every chunk that has clusters with `place_chunk`, which gets
/// where the chunk starts (where the text before it ended, unless it
/// has a position) and returns where the text after it goes on (`None`:
/// where it was). Each chunk's decorations are drawn after it.
fn place_chunks(
    text: &SvgText,
    chunks: &[Chunk],
    fonts: &FontSet<'_>,
    mut place_chunk: impl FnMut(&Chunk, (f32, f32), &mut Placing) -> Option<(f32, f32)>,
) -> LaidText {
    let mut p = Placing::default();
    let (mut last_x, mut last_y) = (0.0f32, 0.0f32);
    for chunk in chunks.iter().filter(|c| !c.clusters.is_empty()) {
        let start = (chunk.x.unwrap_or(last_x), chunk.y.unwrap_or(last_y));
        if let Some(end) = place_chunk(chunk, start, &mut p) {
            (last_x, last_y) = end;
        }
        draw_decorations(&mut p.out, text, &p.runs, fonts);
        p.runs.clear();
    }
    sort(&mut p.out);
    p.out
}

/// Places the clusters of every chunk (`resolve_clusters_positions`),
/// with anchors, baselines, decorations and the ink box.
fn place(text: &SvgText, chunks: &[Chunk], fonts: &FontSet<'_>) -> LaidText {
    place_chunks(text, chunks, fonts, |chunk, (x0, y0), p| {
        if let Some(path) = chunk.path {
            return place_on_path(text, chunk, &text.paths[usize::from(path)], fonts, p);
        }
        let mut x = anchored(chunk);
        let mut y = 0.0;
        for c in &chunk.clusters {
            let ch = text.chars[c.char];
            x += ch.dx;
            y += ch.dy;
            let shifted = ch.dx != 0.0 || ch.dy != 0.0 || ch.rotate != 0.0;
            let span = &text.spans[c.span];
            let font = fonts.get(c.font);
            let down = baseline_drop(span, font);
            let origin = (x0 + x, y0 + y + down);
            p.put((c, span, font), origin, ch.rotate);
            decorate(&mut p.runs, c, shifted, origin, ch.rotate);
            x += c.advance;
        }
        Some((x0 + x, y0 + y))
    })
}

/// Places the clusters of a chunk on a path (`resolve_clusters_positions_path`):
/// the middle of each cluster's advance on the path, turned to the
/// path's direction there, and moved off it by its baseline and `dy`.
/// The chunk starts at `startOffset` plus its x, with its anchor; `dx`
/// moves along the path. A cluster whose middle is off the path is not
/// drawn, and each cluster has its own decorations. Returns where text
/// after it goes on, as `usvg` does: past the last cluster drawn.
fn place_on_path(
    text: &SvgText,
    chunk: &Chunk,
    path: &TextPathSpec,
    fonts: &FontSet<'_>,
    p: &mut Placing,
) -> Option<(f32, f32)> {
    let mut along = chunk.x.unwrap_or(0.0) + path.start + anchored(chunk);
    let mut dy = 0.0;
    let mut end = None;
    for c in &chunk.clusters {
        let ch = text.chars[c.char];
        along += ch.dx;
        dy += ch.dy;
        let half = c.advance / 2.0;
        if let Some(at) = path.geom.at(along + half) {
            let span = &text.spans[c.span];
            let font = fonts.get(c.font);
            let (ux, uy) = geom::rotate((-half, dy + baseline_drop(span, font)), at.angle);
            let origin = (at.x + ux, at.y + uy);
            let rotate = at.angle + ch.rotate;
            p.put((c, span, font), origin, rotate);
            decorate(&mut p.runs, c, true, origin, rotate);
            end = Some((at.x + c.advance, at.y));
        }
        along += c.advance;
    }
    end
}

/// Places the clusters of every chunk of vertical text. A chunk's line
/// goes down from its position (up in `sideways-lr`) on the column's
/// centre: sideways clusters lie on it as horizontal text turned a
/// quarter, on their baseline (central unless set), and upright ones
/// stand on the centre, moved across as far as their baseline is from
/// the central one. `dx` and `dy` move across and along as they move x
/// and y; the anchor and `textLength` act along the line.
fn place_vertical(text: &SvgText, chunks: &[Chunk], fonts: &FontSet<'_>) -> LaidText {
    let line = text.writing.line_angle();
    place_chunks(text, chunks, fonts, |chunk, (x0, y0), p| {
        if let Some(path) = chunk.path {
            let path = &text.paths[usize::from(path)];
            return place_vertical_on_path(text, chunk, path, fonts, p);
        }
        // A point `along` the line and `across` it (down from the glyphs'
        // view), in user space, after the shifts so far.
        let at = |along: f32, across: f32, (sx, sy): (f32, f32)| {
            let (px, py) = geom::rotate((along, across), line);
            (x0 + sx + px, y0 + sy + py)
        };
        let mut along = anchored(chunk);
        let mut shift = (0.0f32, 0.0f32);
        for c in &chunk.clusters {
            let ch = text.chars[c.char];
            shift.0 += ch.dx;
            shift.1 += ch.dy;
            let shifted = ch.dx != 0.0 || ch.dy != 0.0 || ch.rotate != 0.0;
            let span = &text.spans[c.span];
            let font = fonts.get(c.font);
            let down = baseline_drop(span, font);
            let baseline = at(along, down, shift);
            if c.upright {
                let origin = at(along, down - central_drop(span, font), shift);
                p.put((c, span, font), origin, ch.rotate);
            } else {
                p.put((c, span, font), baseline, line + ch.rotate);
            }
            decorate(&mut p.runs, c, shifted, baseline, line);
            along += c.advance;
        }
        Some(at(along, 0.0, shift))
    })
}

/// Places the clusters of a chunk of vertical text on a path, as
/// [`place_on_path`] places horizontal ones: the line runs along the
/// path, each cluster at the middle of its advance on it, sideways ones
/// turned to the path's direction and upright ones standing across it,
/// a quarter turn back. `dy` moves along the path and `dx` across it, as
/// they move along and across a vertical line.
fn place_vertical_on_path(
    text: &SvgText,
    chunk: &Chunk,
    path: &TextPathSpec,
    fonts: &FontSet<'_>,
    p: &mut Placing,
) -> Option<(f32, f32)> {
    let mut along = chunk.y.unwrap_or(0.0) + path.start + anchored(chunk);
    let mut across = 0.0;
    let mut end = None;
    for c in &chunk.clusters {
        let ch = text.chars[c.char];
        along += ch.dy;
        across += ch.dx;
        let half = c.advance / 2.0;
        if let Some(at) = path.geom.at(along + half) {
            let span = &text.spans[c.span];
            let font = fonts.get(c.font);
            let down = baseline_drop(span, font);
            let point = |off: f32| {
                let (ux, uy) = geom::rotate((-half, across + off), at.angle);
                (at.x + ux, at.y + uy)
            };
            let baseline = point(down);
            if c.upright {
                let origin = point(down - central_drop(span, font));
                p.put((c, span, font), origin, at.angle - 90.0 + ch.rotate);
            } else {
                p.put((c, span, font), baseline, at.angle + ch.rotate);
            }
            decorate(&mut p.runs, c, true, baseline, at.angle);
            end = Some((at.x + c.advance, at.y));
        }
        along += c.advance;
    }
    end
}

/// Each probe draws one paint style: stable sorts keep the order.
fn sort(out: &mut LaidText) {
    out.glyphs.sort_by_key(|g| g.paint);
    out.decorations.sort_by_key(|d| d.paint);
    out.images.sort_by_key(|i| i.paint);
}

/// Adds cluster `c`, on its baseline at `origin` turned `rotate`, to the
/// decoration runs: it continues the last one unless it is shifted or of
/// another span.
fn decorate(
    runs: &mut Vec<DecorationRun>,
    c: &Cluster,
    shifted: bool,
    origin: (f32, f32),
    rotate: f32,
) {
    match runs.last_mut() {
        Some(r) if r.span == c.span && !shifted => r.width += c.advance,
        _ => runs.push(DecorationRun {
            span: c.span,
            font: c.font,
            origin,
            rotate,
            width: c.advance,
        }),
    }
}

/// The underlines, overlines and lines-through of `runs`, into `out`.
fn draw_decorations(
    out: &mut LaidText,
    text: &SvgText,
    runs: &[DecorationRun],
    fonts: &FontSet<'_>,
) {
    for run in runs {
        let span = &text.spans[run.span];
        let font = fonts.get(run.font);
        let (rx, ry) = run.origin;
        let (rotate, w) = (run.rotate, run.width);
        let m = font.metrics();
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
                let (px, py) = geom::rotate(p, rotate);
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
}

/// The box of glyph `id`'s ink, in font units, y up: its outline, or the
/// em box of a colour glyph without one.
fn ink_box(font: &LoadedFont<'_>, id: u16) -> Option<[f32; 4]> {
    let m = font.metrics();
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
