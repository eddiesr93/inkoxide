//! The checks of images: their sources, ids, data and placement.

use super::{Checker, MAX_ALT, MAX_PAGE_EDGE, MAX_PERCENT};
use crate::error::ValidationCode as Code;
use crate::images::{ImageCrateDecoder, ImageData};
use crate::schema::{ImageSource, LengthPercent, ObjectPosition};

impl Checker<'_> {
    /// An image's source: an id of the images given (when they are), or
    /// inline `data`.
    pub(super) fn image_source(&mut self, source: &ImageSource) {
        match source {
            ImageSource::Ref(id) => self.image_ref("src", id),
            ImageSource::Inline(inline) => match inline.data() {
                Ok(data) => self.image_data("data", data, None),
                Err(e) => self.err(Code::InvalidImage, Some("data"), e),
            },
        }
    }

    /// An image's alternative text, at `alt`: at most [`MAX_ALT`]
    /// characters.
    pub(super) fn alt(&mut self, alt: Option<&str>) {
        if alt.is_some_and(|a| a.chars().count() > MAX_ALT) {
            self.err(
                Code::InvalidValue,
                Some("alt"),
                format!("alternative text is at most {MAX_ALT} characters"),
            );
        }
    }

    /// Image `id` at `field`: an id of an image given, when they are.
    pub(super) fn image_ref(&mut self, field: &str, id: &str) {
        if !self.id(Some(field), id) {
            return; // reported by `id`
        }
        let Some(images) = self.images else { return };
        let Some(data) = images.get(id) else {
            self.err(
                Code::UnknownImage,
                Some(field),
                format!("image `{id}` was not provided in the render options"),
            );
            return;
        };
        self.image_data(field, data, Some(id));
    }

    /// An image's bytes must decode: its header first, which is cheap and
    /// tells a too large image apart, then the image itself. The decode is
    /// cached, so the render that follows reuses it.
    pub(super) fn image_data(&mut self, field: &str, data: &ImageData, id: Option<&str>) {
        let message = |e: &dyn std::fmt::Display| match id {
            Some(id) => format!("image `{id}`: {e}"),
            None => e.to_string(),
        };
        if let Err(e) = data.probed() {
            self.err(probe_code(&e), Some(field), message(&e));
            return;
        }
        match data.decoded(&ImageCrateDecoder) {
            Err(e) => self.err(Code::InvalidImage, Some(field), message(&e)),
            // Text is laid out with the document's fonts.
            Ok(img) if self.doc.fonts.is_empty() && img.has_text() => self.err(
                Code::InvalidImage,
                Some(field),
                message(
                    &"the SVG has text, and the document declares no fonts to draw it with; \
                      declare one in `fonts`",
                ),
            ),
            Ok(img) if !self.profile.transparency() && self.conflict.is_none() => {
                let what = *self
                    .image_transparency
                    .entry(data.key())
                    .or_insert_with(|| crate::pdf::image_transparency(&img, self.profile));
                if let Some(what) = what {
                    let image = id.map_or("the image".into(), |id| format!("image `{id}`"));
                    self.transparent(Some(field), &format!("{image} ({what})"));
                }
            }
            Ok(_) => {}
        }
    }

    /// Bounds both axes of an image's `objectPosition` (untrusted input: a
    /// huge percentage times the free space would overflow `f32`).
    pub(super) fn object_position(&mut self, p: &ObjectPosition) {
        self.at("objectPosition", |c| {
            for (axis, v) in [("x", p.x), ("y", p.y)] {
                match v {
                    LengthPercent::Points(v) if !(v.abs() <= MAX_PAGE_EDGE) => c.err(
                        Code::OutOfRange,
                        Some(axis),
                        format!("points must be within [-{MAX_PAGE_EDGE}, {MAX_PAGE_EDGE}]"),
                    ),
                    LengthPercent::Percent(f) if !(f.abs() <= MAX_PERCENT) => {
                        let max = MAX_PERCENT * 100.0;
                        c.err(
                            Code::OutOfRange,
                            Some(axis),
                            format!("percentages must be within [-{max}%, {max}%]"),
                        );
                    }
                    _ => {}
                }
            }
        });
    }
}

/// The code for an image that failed its header probe.
fn probe_code(e: &crate::images::ProbeError) -> Code {
    match e {
        crate::images::ProbeError::Invalid(_) => Code::InvalidImage,
        crate::images::ProbeError::TooLarge(_) => Code::ImageTooLarge,
    }
}
