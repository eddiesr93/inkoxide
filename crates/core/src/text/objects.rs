//! Inline objects: images in a line of text. Each is one glyph-like
//! cluster of its own segment (flag `GLYPH_OBJECT`, as wide as the image),
//! so line breaking, `textWrap: pretty`, line boxes and ellipses treat it
//! as any glyph; line metrics and the painter look up the object.
//!
//! An object is an image run (`TextRun::image`). Its size is the run's
//! `width` and `height`, one of them from the image's aspect ratio when
//! only the other is set, and 1em high (the run's `size`) when neither is.

use std::collections::HashMap;

use crate::fonts::FontSet;
use crate::images::ImageKey;
use crate::schema::{ImageFit, ObjectPosition, RunAlign, TextRun};

/// The images available to inline objects, by run.
#[derive(Debug, Default)]
pub(crate) struct InlineObjects {
    runs: HashMap<usize, Source>,
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
    pub fn new(runs: HashMap<usize, Source>) -> Self {
        Self { runs }
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
            fit: image.fit,
            position: image.object_position,
            align: run.vertical_align.unwrap_or_default(),
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
    pub fit: ImageFit,
    pub position: ObjectPosition,
    pub align: RunAlign,
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
