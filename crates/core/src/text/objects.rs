//! Inline objects: images in a line of text. Each is one glyph-like
//! cluster of its own segment (flag `GLYPH_OBJECT`, as wide as the image),
//! so line breaking, `textWrap: pretty`, line boxes and ellipses treat it
//! as any glyph; line metrics and the painter look up the object.
//!
//! An object is an image run (`TextRun::image`), or an emoji sequence the
//! document's `emojiImages` draws as an image. An image run's size is the
//! run's `width` and `height`, one of them from the image's aspect ratio
//! when only the other is set, and 1em high (the run's `size`) when
//! neither is. An emoji image is 1em high, its bottom 0.1em below the
//! run's baseline (raised or lowered by `verticalAlign`, as the run's
//! glyphs are), and its alternative text is the sequence.

use std::collections::HashMap;
use std::ops::Range;

use crate::fonts::FontSet;
use crate::images::{ImageKey, Placement};
use crate::schema::{RunAlign, TextRun};

/// The images available to inline objects: by run, and by emoji sequence
/// (without U+FE0F).
#[derive(Debug, Default)]
pub(crate) struct InlineObjects {
    runs: HashMap<usize, Source>,
    emoji: HashMap<String, Source>,
}

/// An `emojiImages` key: the sequence without its emoji presentation
/// selectors, which writers add or leave out.
pub(crate) fn emoji_key(sequence: &str) -> String {
    sequence.chars().filter(|&c| c != '\u{FE0F}').collect()
}

/// An image and its intrinsic size, in points.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source {
    pub key: ImageKey,
    pub intrinsic: (f32, f32),
}

/// The identity of a run: its address in the document.
pub(crate) fn run_key(r: &TextRun) -> usize {
    std::ptr::from_ref::<TextRun>(r) as usize
}

impl InlineObjects {
    pub fn new(runs: HashMap<usize, Source>, emoji: HashMap<String, Source>) -> Self {
        Self { runs, emoji }
    }

    /// The emoji of `text` (a run's text) drawn as images, by byte range:
    /// each emoji presentation sequence with an image, or, for a ZWJ
    /// sequence without one, each of its parts with an image.
    pub fn emoji_in(&self, text: &str, run: &TextRun) -> Vec<(Range<usize>, InlineObject)> {
        if self.emoji.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for q in super::emoji::sequences(text)
            .into_iter()
            .filter(|q| q.emoji)
        {
            let seq = &text[q.range.clone()];
            if let Some(object) = self.emoji_object(seq, run) {
                out.push((q.range, object));
                continue;
            }
            let mut at = q.range.start;
            for part in seq.split('\u{200D}') {
                if let Some(object) = self.emoji_object(part, run) {
                    out.push((at..at + part.len(), object));
                }
                at += part.len() + '\u{200D}'.len_utf8();
            }
        }
        out
    }

    fn emoji_object(&self, seq: &str, run: &TextRun) -> Option<InlineObject> {
        let src = self.emoji.get(&emoji_key(seq))?;
        let (w, h) = size(src.intrinsic, None, None, run.size);
        Some(InlineObject {
            key: src.key,
            width: w,
            height: h,
            placement: Placement::FILL,
            align: RunAlign::Points(-0.1 * run.size),
            shifted: true,
            alt: Some(seq.into()),
        })
    }

    /// The object of image run `run`.
    pub fn of_run(&self, run: &TextRun) -> Option<InlineObject> {
        let image = run.image.as_deref()?;
        let src = self.runs.get(&run_key(run))?;
        let (w, h) = size(src.intrinsic, image.width, image.height, run.size);
        Some(InlineObject {
            key: src.key,
            width: w,
            height: h,
            placement: Placement::new(image.fit, image.object_position),
            align: run.vertical_align.unwrap_or_default(),
            shifted: false,
            alt: image.alt.as_deref().map(Box::from),
        })
    }
}

/// An object's size: an explicit side wins, and the other follows the
/// aspect ratio; with neither, 1em high.
fn size((iw, ih): (f32, f32), width: Option<f32>, height: Option<f32>, em: f32) -> (f32, f32) {
    let ratio = if iw > 0.0 && ih > 0.0 { iw / ih } else { 1.0 };
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w / ratio),
        (None, Some(h)) => (h * ratio, h),
        (None, None) => (em * ratio, em),
    }
}

/// An image in a paragraph's line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InlineObject {
    pub key: ImageKey,
    pub width: f32,
    pub height: f32,
    pub placement: Placement,
    pub align: RunAlign,
    /// Whether `align` places it from the run's baseline as its
    /// `verticalAlign` moves it, as the run's glyphs are (an emoji image),
    /// rather than from the line's baseline (an image run, which
    /// `verticalAlign` places itself).
    pub shifted: bool,
    /// What text extraction gets for it.
    pub alt: Option<Box<str>>,
}

impl InlineObject {
    /// How far the object reaches above and below the line's baseline, in
    /// points, for an object of run `run`; `None` for `top` and `bottom`,
    /// which depend on the line.
    pub fn extent(&self, run: &TextRun, fonts: &FontSet<'_>) -> Option<(f32, f32)> {
        let h = self.height;
        let font = fonts.key(&run.font).map(|k| fonts.get(k));
        let metric = |f: fn(&crate::fonts::FontMetrics) -> f32, default: f32| {
            font.map_or(default, |font| f(&font.data.metrics) * font.scale(run.size))
        };
        let bottom = match self.align {
            RunAlign::Top | RunAlign::Bottom => return None,
            RunAlign::Baseline => 0.0,
            RunAlign::Super | RunAlign::Sub => super::baseline_shift(run, fonts),
            RunAlign::Points(v) => v,
            RunAlign::Middle => metric(|m| m.x_height, run.size / 2.0) / 2.0 - h / 2.0,
            RunAlign::TextTop => metric(|m| m.ascender, run.size) - h,
            RunAlign::TextBottom => metric(|m| m.descender, 0.0),
        };
        let bottom = if self.shifted {
            bottom + super::baseline_shift(run, fonts)
        } else {
            bottom
        };
        Some((h + bottom, -bottom))
    }
}

#[cfg(test)]
mod tests {
    use super::size;

    #[test]
    fn sizes_follow_the_aspect_ratio() {
        let image = (40.0, 20.0);
        assert_eq!(size(image, Some(10.0), Some(30.0), 12.0), (10.0, 30.0));
        assert_eq!(size(image, Some(10.0), None, 12.0), (10.0, 5.0));
        assert_eq!(size(image, None, Some(10.0), 12.0), (20.0, 10.0));
        // 1em high by default.
        assert_eq!(size(image, None, None, 12.0), (24.0, 12.0));
        // A degenerate image is square.
        assert_eq!(size((0.0, 0.0), None, None, 12.0), (12.0, 12.0));
    }
}
