//! Text in SVG images, before fonts: what each `<text>` element says, read
//! once per SVG and cached with it.
//!
//! `usvg` is built without its text support, so it converts nothing of a
//! `<text>`. This module reads text elements from the XML instead: the
//! cascade of the text properties (fonts, spacing, anchors, baselines,
//! decorations), the characters after `xml:space` processing, their
//! positions, and the paint declared on each span. Laying the characters
//! out needs the render's fonts (`text/svg.rs`); the result replaces each
//! element with a placeholder path `usvg` converts like any other.
//!
//! Only `simplecss` and `svgtypes` read CSS and values here, the same
//! versions `usvg` uses, so selectors and values behave as they do there.

mod extract;
mod rewrite;
#[cfg(test)]
mod survival;

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

pub(crate) use extract::{extract, TextError, DEFAULT_FONT_SIZE};
pub(crate) use rewrite::{index_bits, most_bytes, rewrite, worst_case, Probes};

/// `v` escaped for an XML attribute in double quotes, onto `out`.
fn push_escaped(out: &mut String, v: &str) {
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// The text elements of one SVG document.
#[derive(Debug, Default)]
pub(crate) struct SvgTextDoc {
    pub texts: Vec<SvgText>,
    pub form: ProbeForm,
}

impl SvgTextDoc {
    /// The most probes the placeholders of these texts may hold: one per
    /// paint style.
    pub fn most_probes(&self) -> usize {
        self.texts.iter().map(|t| t.paints.len()).sum()
    }

    /// Approximate bytes these texts hold, each font family list the
    /// spans share counted once.
    pub fn heap_size(&self) -> usize {
        let strings = |v: &[String]| {
            v.iter()
                .map(|s| s.capacity() + size_of::<String>())
                .sum::<usize>()
        };
        let mut lists = HashSet::new();
        let mut size =
            self.form.group_paint.capacity() + self.texts.capacity() * size_of::<SvgText>();
        for t in &self.texts {
            size += t.attrs.capacity()
                + t.effects.capacity()
                + t.chars.capacity() * size_of::<SvgChar>()
                + t.spans.capacity() * size_of::<SvgSpan>()
                + t.paints.capacity() * size_of::<String>()
                + strings(&t.paints)
                + t.adjust.capacity() * size_of::<LengthAdjust>();
            for s in &t.spans {
                size += s.shift.capacity() * size_of::<Shift>();
                if lists.insert(Arc::as_ptr(&s.families).cast::<u8>()) {
                    size += strings(&s.families);
                }
            }
        }
        size
    }
}

/// How the placeholders of one document are written. `usvg` applies an
/// `!important` rule over a placeholder's `style` (the first `!important`
/// declaration it meets wins), so a probe is an element no such rule names
/// by type, when there is one: a rule that matches any element applies to
/// the text in a browser too.
#[derive(Debug, Clone)]
pub(crate) struct ProbeForm {
    pub shape: ProbeShape,
    /// The element that groups the probes of a text with several paint
    /// styles: `a`, or `g`.
    pub group: &'static str,
    /// `inherit` for each paint property a style sheet declares, which a
    /// group holds so that a rule about its element does not reach the
    /// probes inside it.
    pub group_paint: String,
}

impl Default for ProbeForm {
    fn default() -> Self {
        Self {
            shape: ProbeShape::Path,
            group: "a",
            group_paint: String::new(),
        }
    }
}

/// The element a probe is. Both convert to the same four sides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ProbeShape {
    #[default]
    Path,
    Polygon,
}

impl ProbeShape {
    pub fn tag(self) -> &'static str {
        match self {
            ProbeShape::Path => "path",
            ProbeShape::Polygon => "polygon",
        }
    }

    /// The attribute that holds its geometry.
    pub fn attributes(self) -> &'static [&'static str] {
        match self {
            ProbeShape::Path => &["d"],
            ProbeShape::Polygon => &["points"],
        }
    }
}

/// One `<text>` element, ready to lay out.
#[derive(Debug, Clone)]
pub(crate) struct SvgText {
    /// The element's bytes in the source.
    pub range: Range<usize>,
    /// Attributes the placeholder keeps (`id`, and those a `switch`
    /// tests), XML-escaped, each with a leading space.
    pub attrs: String,
    /// Declarations of the element's own effects for the placeholder's
    /// `style` (`transform`, `opacity`, `clip-path`, `mask`, `filter`,
    /// `display`, …), with their initial values where it declares none,
    /// each followed by `;`.
    pub effects: String,
    /// Inside a `clipPath`, where the placeholder holds paths only.
    pub in_clip: bool,
    /// The characters after white space processing, in logical order.
    pub chars: Vec<SvgChar>,
    /// The elements that hold characters directly.
    pub spans: Vec<SvgSpan>,
    /// The paint declarations of each distinct span style (see
    /// [`SvgSpan::paint`]), each followed by `;`.
    pub paints: Vec<String>,
    /// `textLength` on the text and its spans, innermost first.
    pub adjust: Vec<LengthAdjust>,
    /// `direction: rtl` on the text element.
    pub rtl: bool,
}

/// A character and where it goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SvgChar {
    pub c: char,
    /// Index in [`SvgText::spans`].
    pub span: u32,
    /// Absolute positions; either one starts a new chunk.
    pub x: Option<f32>,
    pub y: Option<f32>,
    /// Shifts relative to where the character would be.
    pub dx: f32,
    pub dy: f32,
    /// Degrees, clockwise.
    pub rotate: f32,
    /// In a span that is not displayed (`display: none`): it takes a
    /// position from the lists but is not drawn.
    pub hidden: bool,
}

/// The resolved text properties of an element that holds characters.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SvgSpan {
    /// `font-family`, in order of preference, shared by the spans that
    /// inherit the same declaration.
    pub families: Arc<[String]>,
    pub weight: u16,
    pub italic: bool,
    /// Font size in user units; a span whose size is not finite and
    /// positive is not drawn.
    pub size: f32,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub anchor: Anchor,
    /// `alignment-baseline` when set, `dominant-baseline` otherwise.
    pub baseline: Baseline,
    /// `baseline-shift` of the span and the spans around it inside the
    /// text, innermost first.
    pub shift: Vec<Shift>,
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    /// Index in [`SvgText::paints`].
    pub paint: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

/// A baseline the text's position is on, as `usvg` synthesises it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Baseline {
    Alphabetic,
    Middle,
    Central,
    Hanging,
    Mathematical,
    TextBeforeEdge,
    TextAfterEdge,
    Ideographic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Shift {
    Sub,
    Super,
    /// User units, up.
    Length(f32),
}

/// `textLength` over a range of characters.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LengthAdjust {
    pub chars: Range<usize>,
    pub length: f32,
    /// `lengthAdjust="spacingAndGlyphs"`: glyphs are stretched too.
    pub glyphs: bool,
}
