//! The checks of text runs: their images, emoji images, font sizes and
//! OpenType features.

use super::{
    Checker, MAX_DECORATION_THICKNESS, MAX_EMOJI_SEQUENCE, MAX_FEATURE_SETS, MAX_FEATURE_VALUE,
    MAX_FONT_FEATURES, MAX_FONT_SIZE, MAX_LETTER_SPACING, MAX_PAGE_EDGE, MAX_VARIATIONS,
    MAX_VARIATION_VALUE,
};
use crate::error::ValidationCode as Code;
use crate::schema::{FontFeature, RunAlign, RunImage, TextRun};

impl Checker<'_> {
    /// An `emojiImages` entry: its key is one emoji sequence, of at most
    /// [`MAX_EMOJI_SEQUENCE`] bytes, and its image decodes.
    pub(super) fn emoji_image(&mut self, sequence: &str, id: &str) {
        let one = crate::text::emoji_sequences(sequence);
        let whole = matches!(&one[..], [q] if q.range == (0..sequence.len()));
        if !whole || sequence.len() > MAX_EMOJI_SEQUENCE {
            self.err(
                Code::InvalidValue,
                Some(sequence),
                format!("a key of `emojiImages` is one emoji sequence of at most {MAX_EMOJI_SEQUENCE} bytes in UTF-8"),
            );
        }
        self.image_ref(sequence, id);
    }

    /// A font declaration's `variations`: at most [`MAX_VARIATIONS`] axes,
    /// each once, with a tag of four printable ASCII characters and a value
    /// within ±[`MAX_VARIATION_VALUE`].
    pub(super) fn variations(&mut self, variations: &[crate::schema::FontVariation]) {
        let field = Some("variations");
        if variations.len() > MAX_VARIATIONS {
            self.err(
                Code::OutOfRange,
                field,
                format!("a font takes at most {MAX_VARIATIONS} variation axes"),
            );
            return;
        }
        for (i, v) in variations.iter().enumerate() {
            if !(v.tag.len() == 4 && v.tag.bytes().all(|b| (b' '..=b'~').contains(&b))) {
                self.err(
                    Code::InvalidValue,
                    field,
                    format!(
                        "{:?} is not an axis tag: four printable ASCII characters, such as \"wght\"",
                        v.tag
                    ),
                );
                return;
            }
            if variations[..i].iter().any(|w| w.tag == v.tag) {
                self.err(
                    Code::InvalidValue,
                    field,
                    format!("axis \"{}\" is given twice", v.tag),
                );
                return;
            }
            if !(v.value.abs() <= MAX_VARIATION_VALUE) {
                self.err(
                    Code::OutOfRange,
                    field,
                    format!(
                        "axis \"{}\": a value is within ±{MAX_VARIATION_VALUE}",
                        v.tag
                    ),
                );
                return;
            }
        }
    }

    /// A run's `image`: its source and placement, as an image node's, a
    /// size within a page edge, and bounded alternative text.
    pub(super) fn run_image(&mut self, image: &RunImage) {
        self.object_position(&image.object_position);
        self.image_source(&image.source);
        for (field, v) in [("width", image.width), ("height", image.height)] {
            if v.is_some_and(|v| !(v > 0.0 && v <= MAX_PAGE_EDGE)) {
                self.err(
                    Code::OutOfRange,
                    Some(field),
                    format!("must be within (0, {MAX_PAGE_EDGE}] points"),
                );
            }
        }
        self.alt(image.alt.as_deref());
    }

    pub(super) fn run(&mut self, r: &TextRun) {
        if let Some(link) = &r.link {
            self.link(link);
        }
        match (&r.text, &r.placeholder, &r.image) {
            (Some(_), Some(_), _) => self.err(
                Code::InvalidValue,
                Some("placeholder"),
                "a run takes either `text` or `placeholder`, not both",
            ),
            (Some(_), None, Some(_)) | (None, Some(_), Some(_)) => self.err(
                Code::InvalidValue,
                Some("image"),
                "a run takes one of `text`, `placeholder` and `image`",
            ),
            (None, None, None) if r.note.is_none() => self.err(
                Code::MissingField,
                Some("text"),
                "a run requires `text` (or a `placeholder`, an `image` or a `note`)",
            ),
            (Some(_), _, _) | (_, Some(_), _) | (_, _, Some(_)) if r.note.is_some() => self.err(
                Code::InvalidValue,
                Some("note"),
                "a run takes one of `text`, `placeholder`, `image` and `note`",
            ),
            _ => {}
        }
        if let Some(image) = &r.image {
            self.at("image", |c| c.run_image(image));
        }
        if let Some(note) = &r.note {
            self.at("note", |c| c.note(note));
        }
        match r.vertical_align {
            Some(RunAlign::Points(v)) if !(v.abs() <= MAX_PAGE_EDGE) => self.err(
                Code::OutOfRange,
                Some("verticalAlign"),
                format!("points must be within [-{MAX_PAGE_EDGE}, {MAX_PAGE_EDGE}]"),
            ),
            Some(RunAlign::Baseline | RunAlign::Super | RunAlign::Sub) | None => {}
            Some(_) if r.image.is_none() => self.err(
                Code::InvalidValue,
                Some("verticalAlign"),
                "only `baseline`, `super` and `sub` apply to text; the other values place an image run",
            ),
            Some(_) => {}
        }
        self.font_ref(Some("font"), &r.font);
        self.font_size(r.size);
        if let Some(t) = r.decoration_thickness {
            if !(t > 0.0 && t <= MAX_DECORATION_THICKNESS) {
                self.err(
                    Code::OutOfRange,
                    Some("decorationThickness"),
                    format!("must be within (0, {MAX_DECORATION_THICKNESS}]"),
                );
            }
        }
        if let Some(col) = &r.decoration_color {
            self.color(Some("decorationColor"), col);
        }
        if let Some(col) = &r.color {
            self.color(Some("color"), col);
        }
        if let Some(ls) = r.letter_spacing {
            if !(ls.abs() <= MAX_LETTER_SPACING) {
                self.err(
                    Code::OutOfRange,
                    Some("letterSpacing"),
                    format!("letter spacing must be within ±{MAX_LETTER_SPACING} points"),
                );
            }
        }
        if let Some(features) = &r.font_features {
            self.font_features(features);
        }
    }

    pub(super) fn font_features(&mut self, features: &[FontFeature]) {
        let field = Some("fontFeatures");
        if features.len() > MAX_FONT_FEATURES {
            self.err(
                Code::OutOfRange,
                field,
                format!("a run takes at most {MAX_FONT_FEATURES} font features"),
            );
            return;
        }
        let mut sorted = features.to_vec();
        sorted.sort_by(|a, b| a.tag.cmp(&b.tag));
        for (i, f) in sorted.iter().enumerate() {
            if !(f.tag.len() == 4 && f.tag.bytes().all(|b| (b' '..=b'~').contains(&b))) {
                self.err(
                    Code::InvalidValue,
                    field,
                    format!(
                        "{:?} is not a feature tag: four printable ASCII characters, such as \"tnum\"",
                        f.tag
                    ),
                );
                return;
            }
            if f.value > MAX_FEATURE_VALUE {
                self.err(
                    Code::OutOfRange,
                    field,
                    format!(
                        "the value of {:?} must be within 0..={MAX_FEATURE_VALUE}",
                        f.tag
                    ),
                );
                return;
            }
            if i > 0 && sorted[i - 1].tag == f.tag {
                self.err(
                    Code::InvalidValue,
                    field,
                    format!("{:?} is set twice", f.tag),
                );
                return;
            }
        }
        if sorted.is_empty() || self.feature_sets.contains(&sorted) {
            return;
        }
        if self.feature_sets.len() >= MAX_FEATURE_SETS {
            self.err(
                Code::OutOfRange,
                field,
                format!(
                    "a document uses at most {MAX_FEATURE_SETS} different sets of font features"
                ),
            );
            return;
        }
        self.feature_sets.insert(sorted);
    }

    /// The `size` of a run or a form field.
    pub(super) fn font_size(&mut self, size: f32) {
        if !(size > 0.0 && size <= MAX_FONT_SIZE) {
            self.err(
                Code::OutOfRange,
                Some("size"),
                format!("font size must be within (0, {MAX_FONT_SIZE}]"),
            );
        }
    }
}
