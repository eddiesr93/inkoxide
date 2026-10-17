//! SVG text converted again with a render's fonts: each text replaced
//! with its placeholder, and the converted trees kept across renders.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::check::{check, Checked};
use super::limits::{too_complex, Meter, Reject, MAX_PROBE_BYTES};
use super::{parse_nested, parse_xml, Nesting};
use crate::images::svg_text::{Probes, SvgText, SvgTextDoc};
use crate::images::ContentKey;
use crate::lru::SharedLru;
use crate::schema::FontFeature;
use crate::text::LaidText;

/// What converting an SVG with text again needs, with a render's fonts:
/// its bytes and text elements, and the same for its nested SVG images
/// with text. An SVG without text of its own has this for its nested ones.
#[derive(Debug)]
pub(crate) struct TextSource {
    pub bytes: Arc<Vec<u8>>,
    pub text: SvgTextDoc,
    pub nested: Vec<TextSource>,
}

impl TextSource {
    /// What converting `bytes`, checked as `checked`, needs; `None` when
    /// neither it nor its nested SVGs have text.
    pub(super) fn new(bytes: Arc<Vec<u8>>, checked: Checked) -> Option<Self> {
        let some = !checked.text.texts.is_empty() || !checked.nested.is_empty();
        some.then_some(Self {
            bytes,
            text: checked.text,
            nested: checked.nested,
        })
    }

    /// Adds the different lists of OpenType features its text and its
    /// nested SVGs' use to `sets`.
    pub fn feature_sets(&self, sets: &mut HashSet<Arc<[FontFeature]>>) {
        let spans = self.text.texts.iter().flat_map(|t| &t.spans);
        sets.extend(
            spans
                .filter(|s| !s.features.is_empty())
                .map(|s| s.features.clone()),
        );
        for n in &self.nested {
            n.feature_sets(sets);
        }
    }

    /// Approximate bytes it holds, its nested SVGs' included.
    pub fn heap_size(&self) -> usize {
        self.nested.iter().map(Self::heap_size).fold(
            self.bytes.capacity() + self.text.heap_size(),
            usize::saturating_add,
        )
    }

    /// The most probes its placeholders and its nested SVGs' hold.
    pub(super) fn most_probes(&self) -> usize {
        self.nested
            .iter()
            .map(Self::most_probes)
            .fold(self.text.most_probes(), usize::saturating_add)
    }
}

/// The text of an SVG converted with a render's fonts: each text laid
/// out, in the order of [`TextSource`] (its own texts, then its nested
/// SVGs' in turn), and the probe that stands for each text and paint.
#[derive(Debug)]
pub(crate) struct ProbedText {
    pub texts: Vec<LaidText>,
    pub probes: Probes,
    /// The size of the SVG without its text: an SVG without a size takes
    /// it from what it draws, which would otherwise depend on the fonts.
    pub size: usvg::Size,
}

/// Rewritten nested SVGs, by their original bytes.
pub(super) type Rewrites = HashMap<ContentKey<Arc<Vec<u8>>>, Arc<Vec<u8>>>;

/// Bytes of rewritten SVG kept with their trees across renders: renders
/// whose fonts lay the text out the same way reuse the conversion.
pub(super) const TEXT_TREES_BUDGET: usize = 32 << 20;

/// Trees converted with text, by every rewritten source they come from.
pub(crate) static TEXT_TREES: SharedLru<ContentKey<Arc<[u8]>>, Arc<usvg::Tree>> =
    SharedLru::new(TEXT_TREES_BUDGET);

/// Converts the SVG of `source` with each text laid out by `lay_out` and
/// replaced with its placeholder (`svg_text/rewrite.rs`), with the same
/// checks as [`parse`]. A conversion of the same rewritten sources is
/// reused, from this render or another.
pub(crate) fn parse_with_text(
    source: &TextSource,
    size: usvg::Size,
    lay_out: &mut dyn FnMut(&SvgText) -> LaidText,
) -> Result<(Arc<usvg::Tree>, ProbedText), String> {
    let mut probed = ProbedText {
        texts: Vec::new(),
        probes: Probes::new(),
        size,
    };
    let mut rewrites = Rewrites::new();
    let mut order = Vec::new();
    let bits = crate::images::svg_text::index_bits(source.most_probes());
    let mut rewriting = Rewriting {
        lay_out,
        probed: &mut probed,
        bits,
        rewrites: &mut rewrites,
        order: &mut order,
    };
    let top = rewriting.all(source).map_err(Reject::message)?;
    // The key: the rewritten SVG, then each rewritten nested SVG after a
    // byte UTF-8 never has.
    let mut key = top.clone().into_bytes();
    for nested in &order {
        key.push(0xFF);
        key.extend_from_slice(nested);
    }
    let size = key.len();
    let key = ContentKey::new(Arc::<[u8]>::from(key));
    if let Some(tree) = TEXT_TREES.get(&key) {
        return Ok((tree, probed));
    }
    let meter = Meter::default();
    let at = Nesting {
        meter: &meter,
        level: 0,
        depth: 0,
        rewrites: Some(&rewrites),
    };
    let parsed = parse_nested(top.as_bytes(), at).map_err(Reject::message);
    if let Some(e) = meter.error.get() {
        return Err(e.clone());
    }
    let tree = Arc::new(parsed?.0);
    // A tree is several times larger than its source.
    let size = size.saturating_mul(9);
    TEXT_TREES.insert(key, tree.clone(), size);
    Ok((tree, probed))
}

/// The SVG of `source` converted with a probe for each text and each of
/// its paint styles, without fonts: what its text is painted with, for
/// checks that do not lay it out (PDF/A-1's transparency check).
pub(crate) fn probe_tree(
    source: &TextSource,
    size: usvg::Size,
) -> Result<(Arc<usvg::Tree>, ProbedText), String> {
    parse_with_text(source, size, &mut |text: &SvgText| {
        LaidText::unit(text.paints.len())
    })
}

/// The SVGs of a [`TextSource`] being rewritten for a render.
pub(super) struct Rewriting<'a> {
    pub(super) lay_out: &'a mut dyn FnMut(&SvgText) -> LaidText,
    pub(super) probed: &'a mut ProbedText,
    /// Bits of each probe corner that carry its number.
    pub(super) bits: u32,
    /// The nested SVGs rewritten, by their original bytes, and in order.
    pub(super) rewrites: &'a mut Rewrites,
    pub(super) order: &'a mut Vec<Arc<Vec<u8>>>,
}

impl Rewriting<'_> {
    /// `source`'s SVG with its texts replaced, its nested ones rewritten
    /// into `rewrites`.
    pub(super) fn all(&mut self, source: &TextSource) -> Result<String, Reject> {
        let text = source_text(&source.bytes);
        probe_bytes(&source.text)?;
        let laid: Vec<LaidText> = source.text.texts.iter().map(&mut *self.lay_out).collect();
        let first = self.probed.texts.len() as u32;
        let probes = (&mut self.probed.probes, self.bits);
        let out = crate::images::svg_text::rewrite(text, &source.text, &laid, first, probes);
        self.probed.texts.extend(laid);
        for nested in &source.nested {
            let r = Arc::new(self.all(nested)?.into_bytes());
            self.order.push(r.clone());
            self.rewrites
                .insert(ContentKey::new(nested.bytes.clone()), r);
        }
        Ok(out)
    }
}

/// An SVG's text, without its byte order mark; empty when it is not
/// UTF-8 (it has passed [`parse_xml`] before).
pub(super) fn source_text(bytes: &[u8]) -> &str {
    let text = std::str::from_utf8(bytes).unwrap_or_default();
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

/// Refuses text whose placeholders would take more than
/// [`MAX_PROBE_BYTES`], before they are written.
pub(super) fn probe_bytes(text: &SvgTextDoc) -> Result<(), Reject> {
    if crate::images::svg_text::most_bytes(text) > MAX_PROBE_BYTES {
        return Err(too_complex(&format!(
            "its text takes more than {} MiB once laid out",
            MAX_PROBE_BYTES >> 20
        )));
    }
    Ok(())
}

/// Checks the document `bytes` becomes once its text is replaced, with
/// the placeholders that cost most (`svg_text::worst_case`): what any
/// render converts costs the checks no more, so a render fails a limit
/// only where the decode does.
pub(super) fn check_placeholders(
    bytes: &[u8],
    text: &SvgTextDoc,
    at: Nesting<'_>,
) -> Result<(), Reject> {
    probe_bytes(text)?;
    let worst = crate::images::svg_text::worst_case(source_text(bytes), text);
    let doc = parse_xml(worst.as_bytes())?;
    check(&doc, at.level, at.depth).map(drop)
}
