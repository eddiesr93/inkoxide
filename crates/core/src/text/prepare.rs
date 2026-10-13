//! Width-independent text preparation: whitespace processing, hyphenation
//! points, font itemisation and shaping.
//!
//! The only user of the shaping library (rustybuzz): its faces, plans and
//! buffers stay private to this file.

use std::collections::HashMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use unicode_linebreak::{linebreaks, BreakOpportunity};

use super::hyphen::Hyphenator;
use super::{
    BreakKind, BreakPoint, Glyph, PageNumbers, Para, Prepared, Segment, GLYPH_INVISIBLE,
    GLYPH_OBJECT, GLYPH_SPACE, GLYPH_TAB,
};
use crate::fonts::{FontKey, FontSet, LoadedFont};
use crate::schema::{Direction, FontFeature, TextRun, WhiteSpace};
use crate::InkoxideError;

/// Soft hyphen: an invisible break opportunity that shows `-` when used.
pub(crate) const SHY: char = '\u{00AD}';

/// Stand-in digits laid out in place of page-number placeholders until the
/// page count is known. Headers, footers and overlays are laid out again
/// on every page with the real numbers ([`fill_numbers`]), and flow text
/// with the numbers of its page; flow text painted on several pages
/// (fixed, or in a table's header rows) keeps this width.
pub(crate) const PLACEHOLDER_TEXT: &str = "999";

/// The text of an image run: U+FFFC OBJECT REPLACEMENT CHARACTER.
const OBJECT_TEXT: &str = "\u{FFFC}";

pub(crate) struct ParagraphOptions<'a> {
    pub white_space: WhiteSpace,
    pub direction: Direction,
    /// Whether runs without their own `hyphenate` are hyphenated.
    pub hyphenate: bool,
    /// Language code to hyphenate in (only set when patterns exist).
    pub lang: Option<&'a str>,
    pub hyphenator: &'a dyn Hyphenator,
    pub runs: &'a [TextRun],
    /// The images of image runs.
    pub objects: &'a super::InlineObjects,
}

/// Combining marks stay in the font of their base character.
fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Tab stops of `pre` and `pre-wrap` text, in spaces (CSS `tab-size`).
const TAB_SIZE: f32 = 8.0;

/// White space that collapses in `normal`, `nowrap` and `pre-line`: the
/// space, tab and line-break characters CSS collapses, and the other
/// control and separator characters, which have no glyph of their own.
/// Other spaces (thin, em, figure, ideographic, no-break, …) keep their
/// width and their line-breaking class.
fn collapses(c: char) -> bool {
    c.is_whitespace() && (c.is_ascii() || c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}

/// Builds the paragraph text from the runs. Returns the text and, per run,
/// its byte range in it.
fn build_text(opts: &ParagraphOptions<'_>) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut ranges = Vec::with_capacity(opts.runs.len());
    // In collapsing modes: whether the text so far ends in a space or a
    // line start (true initially: leading space is dropped).
    let mut last_was_space = true;
    for run in opts.runs {
        let start = text.len();
        let src: &str = match (run.placeholder, &run.image) {
            (Some(_), _) => PLACEHOLDER_TEXT,
            // An image is one object character: it breaks lines like one
            // (UAX #14 class CB) and is neutral in bidi.
            (None, Some(_)) => OBJECT_TEXT,
            (None, None) => run.text.as_deref().unwrap_or(""),
        };
        match opts.white_space {
            WhiteSpace::Pre | WhiteSpace::PreWrap => {
                // Tabs stay: shaping gives them the advance to the next
                // tab stop (`place_tabs`).
                text.extend(src.chars().filter(|&c| c != '\r'));
            }
            WhiteSpace::Normal | WhiteSpace::NoWrap | WhiteSpace::PreLine => {
                for c in src.chars() {
                    if c == '\n' {
                        // Forced break, like CSS `pre-line`. A space just
                        // before it is dropped.
                        if text.ends_with(' ') && text.len() > start {
                            text.pop();
                        }
                        text.push('\n');
                        last_was_space = true;
                    } else if collapses(c) {
                        // A collapsed space stays in the run where it
                        // first occurs (matters for decorations).
                        if !last_was_space {
                            text.push(' ');
                            last_was_space = true;
                        }
                    } else {
                        text.push(c);
                        last_was_space = false;
                    }
                }
            }
        }
        if let Some(lang) = opts.lang {
            if run.hyphenate.unwrap_or(opts.hyphenate)
                && !matches!(opts.white_space, WhiteSpace::Pre | WhiteSpace::NoWrap)
                && run.placeholder.is_none()
                && run.image.is_none()
            {
                let hyphenated = insert_soft_hyphens(&text[start..], opts.hyphenator, lang);
                text.truncate(start);
                text.push_str(&hyphenated);
            }
        }
        ranges.push((start, text.len()));
    }
    (text, ranges)
}

/// Inserts U+00AD at hyphenation points of words longer than five letters.
fn insert_soft_hyphens(s: &str, hyphenator: &dyn Hyphenator, lang: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    let mut word_start: Option<usize> = None;
    let flush = |out: &mut String, word: &str| {
        if word.chars().count() > 5 {
            let mut last = 0;
            for at in hyphenator.hyphenate(word, lang) {
                out.push_str(&word[last..at]);
                out.push(SHY);
                last = at;
            }
            out.push_str(&word[last..]);
        } else {
            out.push_str(word);
        }
    };
    for (i, c) in s.char_indices() {
        if c.is_alphabetic() {
            word_start.get_or_insert(i);
        } else {
            if let Some(ws) = word_start.take() {
                flush(&mut out, &s[ws..i]);
            }
            out.push(c);
        }
    }
    if let Some(ws) = word_start {
        flush(&mut out, &s[ws..]);
    }
    out
}

/// Chooses a font for every character: the run's font if it has the glyph,
/// else the first fallback that has it, else the run's font (`.notdef`).
/// An emoji sequence takes one font for all its characters
/// ([`super::emoji::font`]).
/// An image run is a segment of its own, with its object.
fn itemize(
    text: &str,
    run_ranges: &[(usize, usize)],
    runs: &[TextRun],
    fonts: &FontSet<'_>,
    levels: Option<&[u8]>,
    objects: &super::InlineObjects,
) -> (Vec<Segment>, Vec<super::objects::InlineObject>) {
    let level_at = |i: usize| levels.map_or(0, |l| l[i]);
    let mut segs: Vec<Segment> = Vec::new();
    let mut objs = Vec::new();
    for (ri, (&(start, end), run)) in run_ranges.iter().zip(runs).enumerate() {
        let Some(primary) = fonts.key(&run.font) else {
            continue;
        };
        if let Some(object) = objects.of_run(run).filter(|_| start < end) {
            let mut seg = Segment::new(ri, primary, level_at(start), start..end);
            seg.object = Some(objs.len() as u32);
            objs.push(object);
            segs.push(seg);
            continue;
        }
        let mut current: Option<(FontKey, usize)> = None;
        let mut level = 0u8;
        let mut sequences = super::emoji::sequences(&text[start..end])
            .into_iter()
            .peekable();
        // The end and font of the emoji sequence being itemised.
        let mut sequence: Option<(usize, FontKey)> = None;
        for (off, c) in text[start..end].char_indices() {
            let i = start + off;
            sequence = sequence.filter(|&(e, _)| i < e);
            if let Some(q) = sequences.next_if(|q| q.range.start == off) {
                sequence = Some((start + q.range.end, super::emoji::font(&q, primary, fonts)));
            }
            let neutral = c.is_whitespace() || c == SHY || c.is_control() || is_mark(c);
            let lvl = level_at(i);
            if let Some((f, s)) = current {
                if lvl != level {
                    segs.push(Segment::new(ri, f, level, s..i));
                    current = None;
                }
            }
            level = lvl;
            let font = match current {
                _ if sequence.is_some() => sequence.map_or(primary, |(_, f)| f),
                Some((f, _)) if neutral => f,
                _ if fonts.get(primary).glyph(c).is_some() || neutral => primary,
                _ => fonts
                    .fallbacks
                    .iter()
                    .copied()
                    .find(|&f| fonts.get(f).glyph(c).is_some())
                    .unwrap_or(primary),
            };
            match current {
                Some((f, _)) if f == font => {}
                Some((f, s)) => {
                    segs.push(Segment::new(ri, f, level, s..i));
                    current = Some((font, i));
                }
                None => current = Some((font, i)),
            }
        }
        if let Some((f, s)) = current {
            segs.push(Segment::new(ri, f, level, s..end));
        }
    }
    (segs, objs)
}

/// A shaped glyph in font units, relative to its word.
#[derive(Debug, Clone, Copy)]
struct UnitGlyph {
    id: u16,
    x_advance: i32,
    x_offset: i32,
    y_offset: i32,
    /// Byte offset of the cluster within the word.
    cluster: u32,
}

/// Most glyphs shaping may add, in one render, beyond one per byte of the
/// text shaped: a memory guard like `MAX_PAGES`. Text shapes to about one
/// glyph per byte or fewer (ligatures merge glyphs, and a mark is bytes of
/// its own); a font whose layout tables multiply glyphs would otherwise
/// turn a few kilobytes of text into gigabytes. About 100 MB of glyphs.
pub(crate) const MAX_EXTRA_GLYPHS: u64 = 1 << 22;

/// Why shaping stopped a render, and the id of the font being shaped.
/// Shaping has no error path (the layout engine measures text through a
/// callback), so this unwinds out of it, in place of a panic's own
/// payload; `render` turns it into a font error.
pub(crate) struct ShapingFailure {
    pub font: String,
    pub cause: ShapingCause,
}

pub(crate) enum ShapingCause {
    /// The shaper panicked, as it does on some malformed fonts.
    /// Registration rejects the ones known (see `fonts/layout.rs`).
    Panicked,
    /// The render's text shaped to more than [`MAX_EXTRA_GLYPHS`] extra
    /// glyphs.
    TooManyGlyphs,
}

/// Font, direction, feature set ([`ShapeCache::features`]) and text of a
/// shaped word.
type WordKey = (FontKey, Option<bool>, FeatureSet, Box<str>);

/// Index of a run's OpenType features in [`ShapeCache::features`]; 0 is
/// none. Validation bounds the different sets of a document.
type FeatureSet = u16;

/// Shaping faces of a render's fonts, indexed by [`FontKey`]: parsed once
/// per render and shared by every [`ShapeCache`] (one per shaping thread).
#[derive(Clone)]
pub(super) struct Faces<'a>(Arc<[rustybuzz::Face<'a>]>);

impl<'a> Faces<'a> {
    pub fn load(fonts: &FontSet<'a>) -> Result<Self, InkoxideError> {
        fonts
            .fonts
            .iter()
            .map(|f| {
                let data = f.data;
                // `from_slice` fails only where ttf-parser does, and
                // `FontSet::load` parsed these bytes already.
                rustybuzz::Face::from_slice(&data.bytes, 0).ok_or_else(|| {
                    InkoxideError::Font(format!(
                        "font `{}` cannot be loaded for shaping",
                        data.metrics.post_script_name
                    ))
                })
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

/// Per-thread shaping cache: shape plans per (font, features, direction,
/// script) and shaped words per (font, direction, features, text). Documents repeat words constantly
/// (table cells, numbers, common vocabulary), so shaping word by word and
/// reusing results removes most shaping work.
pub(super) struct ShapeCache<'a> {
    faces: Faces<'a>,
    /// Glyphs the render's text has shaped to beyond one per byte, shared
    /// by the caches of its threads: against [`MAX_EXTRA_GLYPHS`].
    extra: Arc<AtomicU64>,
    plans: HashMap<
        (
            FontKey,
            FeatureSet,
            rustybuzz::Direction,
            Option<rustybuzz::Script>,
        ),
        rustybuzz::ShapePlan,
    >,
    /// Keyed by font, direction (`Some(true)`: right to left; `None`:
    /// guessed from the text), features and text.
    words: HashMap<WordKey, Arc<[UnitGlyph]>>,
    /// The feature sets of the runs shaped so far, by [`FeatureSet`]; the
    /// first is empty.
    features: Vec<Box<[rustybuzz::Feature]>>,
    /// Each run's features, sorted by tag, to their [`FeatureSet`].
    feature_sets: HashMap<Box<[FontFeature]>, FeatureSet>,
}

impl<'a> ShapeCache<'a> {
    pub fn new(faces: Faces<'a>) -> Self {
        Self {
            faces,
            extra: Arc::default(),
            plans: HashMap::new(),
            words: HashMap::new(),
            features: vec![Box::default()],
            feature_sets: HashMap::new(),
        }
    }

    /// The [`FeatureSet`] of `run`'s `fontFeatures`.
    fn feature_set(&mut self, run: &TextRun) -> FeatureSet {
        let Some(list) = run.font_features.as_deref().filter(|f| !f.is_empty()) else {
            return 0;
        };
        // One set whatever the order: validation rejects a tag set twice.
        let mut sorted = list.to_vec();
        sorted.sort_by(|a, b| a.tag.cmp(&b.tag));
        if let Some(&set) = self.feature_sets.get(sorted.as_slice()) {
            return set;
        }
        let features = sorted
            .iter()
            .filter_map(|f| {
                let tag: [u8; 4] = f.tag.as_bytes().try_into().ok()?;
                let tag = rustybuzz::ttf_parser::Tag::from_bytes(&tag);
                Some(rustybuzz::Feature::new(tag, f.value, ..))
            })
            .collect();
        let set = FeatureSet::try_from(self.features.len()).unwrap_or(FeatureSet::MAX);
        self.features.push(features);
        self.feature_sets.insert(sorted.into(), set);
        set
    }

    /// An empty cache for another thread of the same render: the same
    /// faces, and the same count of extra glyphs.
    pub fn sibling(&self) -> Self {
        Self {
            extra: self.extra.clone(),
            ..Self::new(self.faces.clone())
        }
    }

    /// Counts `glyphs` shaped from `bytes` of text; false once the render's
    /// text has shaped to more than [`MAX_EXTRA_GLYPHS`] extra glyphs.
    fn spend(&self, glyphs: usize, bytes: usize) -> bool {
        let extra = glyphs.saturating_sub(bytes) as u64;
        extra == 0 || self.extra.fetch_add(extra, Ordering::Relaxed) + extra <= MAX_EXTRA_GLYPHS
    }

    /// Shapes `word` in `font`, recording glyph → text mappings the first
    /// time a word is seen.
    /// Glyphs come back in logical order (clusters ascending) whatever
    /// the direction; painting reverses right-to-left runs.
    fn word(
        &mut self,
        key: FontKey,
        word: &str,
        rtl: Option<bool>,
        set: FeatureSet,
        unicode: &mut super::UnicodeMap,
    ) -> Arc<[UnitGlyph]> {
        if let Some(g) = self.words.get(&(key, rtl, set, Box::from(word))) {
            return g.clone();
        }
        let face = &self.faces.0[usize::from(key)];
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(word);
        buf.guess_segment_properties();
        if let Some(rtl) = rtl {
            buf.set_direction(if rtl {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
        }
        let reversed = buf.direction() == rustybuzz::Direction::RightToLeft && rtl.is_some();
        let features = self
            .features
            .get(usize::from(set))
            .map_or(&[][..], |f| &f[..]);
        let plan = self
            .plans
            .entry((key, set, buf.direction(), Some(buf.script())))
            .or_insert_with(|| {
                rustybuzz::ShapePlan::new(face, buf.direction(), Some(buf.script()), None, features)
            });
        let shaped = rustybuzz::shape_with_plan(face, plan, buf);
        let infos = shaped.glyph_infos();
        let mut glyphs: Vec<UnitGlyph> = infos
            .iter()
            .zip(shaped.glyph_positions())
            .map(|(i, p)| UnitGlyph {
                id: i.glyph_id as u16,
                x_advance: p.x_advance,
                x_offset: p.x_offset,
                y_offset: p.y_offset,
                cluster: i.cluster,
            })
            .collect();
        if reversed {
            glyphs.reverse();
        }
        let glyphs: Arc<[UnitGlyph]> = glyphs.into();
        // The first glyph of a cluster carries the cluster's text.
        for (k, g) in glyphs.iter().enumerate() {
            let cluster = g.cluster as usize;
            let Some(c) = word[cluster..].chars().next() else {
                continue;
            };
            if c == '\n' || c == SHY || c.is_control() {
                continue;
            }
            if k > 0 && glyphs[k - 1].cluster == g.cluster {
                continue;
            }
            let next = glyphs[k + 1..]
                .iter()
                .map(|n| n.cluster as usize)
                .find(|&n| n != cluster)
                .unwrap_or(word.len());
            let end = next.max(cluster + c.len_utf8()).min(word.len());
            unicode.record(key, g.id, &word[cluster..end]);
        }
        self.words
            .insert((key, rtl, set, Box::from(word)), glyphs.clone());
        glyphs
    }
}

/// Splits text into shaping units: maximal runs of non-spaces, and single
/// spaces and tabs.
fn words(s: &str) -> impl Iterator<Item = (usize, &str)> {
    let b = s.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        if i >= b.len() {
            return None;
        }
        let start = i;
        if matches!(b[i], b' ' | b'\t') {
            i += 1;
        } else {
            while i < b.len() && !matches!(b[i], b' ' | b'\t') {
                i += 1;
            }
        }
        Some((start, &s[start..i]))
    })
}

/// Shapes the paragraph. `unicode` receives glyph → text mappings for the
/// PDF `ToUnicode` `CMaps`.
pub(crate) fn prepare(
    opts: &ParagraphOptions<'_>,
    fonts: &FontSet<'_>,
    cache: &mut ShapeCache<'_>,
    unicode: &mut super::UnicodeMap,
) -> Prepared {
    let (text, run_ranges) = build_text(opts);
    let (levels, rtl) = bidi_levels(&text, opts.direction);
    let bidi = levels
        .as_ref()
        .is_some_and(|l| l.iter().any(|v| v % 2 == 1));
    let (mut segments, objects) = itemize(
        &text,
        &run_ranges,
        opts.runs,
        fonts,
        levels.as_deref(),
        opts.objects,
    );
    let mut glyphs: Vec<Glyph> = Vec::with_capacity(text.len());

    let sets: Vec<FeatureSet> = opts.runs.iter().map(|r| cache.feature_set(r)).collect();
    for seg in &mut segments {
        if let Some(o) = seg.object {
            // One glyph-like cluster, as wide as the image.
            let first = glyphs.len() as u32;
            glyphs.push(Glyph {
                id: 0,
                advance: objects[o as usize].width,
                x_offset: 0.0,
                y_offset: 0.0,
                cluster: seg.text.start,
                flags: GLYPH_OBJECT,
            });
            seg.glyphs = first..first + 1;
            continue;
        }
        let run = &opts.runs[seg.run as usize];
        let set = sets[seg.run as usize];
        let scale = fonts.get(seg.font).scale(run.size);
        let letter_spacing = run.letter_spacing.unwrap_or(0.0);
        let seg_text = &text[seg.text.start as usize..seg.text.end as usize];

        let first = glyphs.len() as u32;
        let dir = levels.is_some().then_some(seg.level % 2 == 1);
        for (off, word) in words(seg_text) {
            let fail = |cause| {
                resume_unwind(Box::new(ShapingFailure {
                    font: fonts.get(seg.font).id.to_string(),
                    cause,
                }))
            };
            let shaped = catch_unwind(AssertUnwindSafe(|| {
                cache.word(seg.font, word, dir, set, unicode)
            }))
            .unwrap_or_else(|_| fail(ShapingCause::Panicked));
            // Every occurrence counts: each copies its glyphs.
            if !cache.spend(shaped.len(), word.len()) {
                fail(ShapingCause::TooManyGlyphs);
            }
            for (k, g) in shaped.iter().enumerate() {
                let c = word[g.cluster as usize..].chars().next().unwrap_or(' ');
                let mut flags = 0;
                if c == ' ' || c == '\u{00A0}' {
                    flags |= GLYPH_SPACE;
                }
                if c == '\n' || c == SHY || c.is_control() {
                    flags |= GLYPH_INVISIBLE;
                }
                if c == '\t' {
                    flags |= GLYPH_TAB;
                }
                // Letter spacing goes after the last glyph of each cluster.
                let cluster_end = shaped.get(k + 1).is_none_or(|n| n.cluster != g.cluster);
                let advance = if flags & GLYPH_INVISIBLE != 0 {
                    0.0
                } else {
                    g.x_advance as f32 * scale + if cluster_end { letter_spacing } else { 0.0 }
                };
                glyphs.push(Glyph {
                    id: g.id,
                    advance,
                    x_offset: g.x_offset as f32 * scale,
                    y_offset: g.y_offset as f32 * scale,
                    cluster: seg.text.start + (off as u32) + g.cluster,
                    flags,
                });
            }
        }
        seg.glyphs = first..glyphs.len() as u32;
    }

    // Break opportunities, as glyph indices where the next line starts.
    let mut breaks = Vec::new();
    let nowrap = matches!(opts.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre);
    let mut gi = 0usize;
    for (pos, kind) in linebreaks(&text) {
        if pos == text.len() {
            break;
        }
        let prev = text[..pos].chars().next_back();
        let kind = match kind {
            BreakOpportunity::Mandatory => BreakKind::Mandatory,
            BreakOpportunity::Allowed if nowrap => continue,
            BreakOpportunity::Allowed if prev == Some(SHY) => BreakKind::Hyphen,
            BreakOpportunity::Allowed => BreakKind::Allowed,
        };
        while gi < glyphs.len() && (glyphs[gi].cluster as usize) < pos {
            gi += 1;
        }
        // Only break where a new cluster starts (never inside a ligature).
        if gi < glyphs.len() && glyphs[gi].cluster as usize == pos {
            breaks.push(BreakPoint {
                glyph: gi as u32,
                kind,
            });
        }
    }

    place_tabs(&mut glyphs, &segments, &breaks, opts.runs, fonts);

    Prepared {
        #[cfg(test)]
        text,
        segments,
        glyphs,
        breaks,
        rtl,
        bidi,
        placeholders: opts.runs.iter().any(|r| r.placeholder.is_some()),
        objects,
    }
}

/// Gives each tab the advance to the next tab stop, as CSS `tab-size: 8`:
/// stops are 8 spaces of the tab's run font (letter spacing included)
/// apart, from the start of the line, and a tab that would advance less
/// than half a `0` goes on to the stop after. Lines start at forced
/// breaks: a line `pre-wrap` wraps keeps the stops of the line it
/// continues.
fn place_tabs(
    glyphs: &mut [Glyph],
    segments: &[Segment],
    breaks: &[BreakPoint],
    runs: &[TextRun],
    fonts: &FontSet<'_>,
) {
    if !glyphs.iter().any(Glyph::is_tab) {
        return;
    }
    let mut line_starts = breaks
        .iter()
        .filter(|b| b.kind == BreakKind::Mandatory)
        .map(|b| b.glyph as usize)
        .peekable();
    let mut x = 0.0f32;
    for seg in segments {
        let run = &runs[seg.run as usize];
        let font = fonts.get(fonts.key(&run.font).unwrap_or(seg.font));
        let scale = font.scale(run.size);
        let advance_of = |c: char| font.glyph(c).map(|g| font.advance(g) * scale);
        let space = advance_of(' ').unwrap_or(run.size / 4.0) + run.letter_spacing.unwrap_or(0.0);
        let stop = TAB_SIZE * space;
        let half_zero = advance_of('0').unwrap_or(run.size / 2.0) / 2.0;
        for i in seg.glyphs.start as usize..seg.glyphs.end as usize {
            while line_starts.next_if(|&at| at <= i).is_some() {
                x = 0.0;
            }
            let g = &mut glyphs[i];
            if g.is_tab() {
                g.advance = 0.0;
                if stop > 0.0 {
                    let mut next = ((x / stop).floor() + 1.0) * stop;
                    if next - x < half_zero {
                        next += stop;
                    }
                    g.advance = next - x;
                }
            }
            x += g.advance;
        }
    }
}

/// The glyphs of number `n` in run `run` of font `font`, left to right,
/// each with its advance (letter spacing included): each digit's glyph
/// from the font's `cmap`, without shaping. What the painter draws for a
/// placeholder, and what [`fill_numbers`] lays out.
pub(super) fn number_glyphs(font: &LoadedFont<'_>, run: &TextRun, n: u32) -> Vec<(u16, f32)> {
    let scale = font.scale(run.size);
    let letter_spacing = run.letter_spacing.unwrap_or(0.0);
    n.to_string()
        .chars()
        .map(|c| {
            let id = font.glyph(c).unwrap_or(0);
            (id, font.advance(id) * scale + letter_spacing)
        })
        .collect()
}

/// The paragraph with the page numbers in place of the stand-in digits of
/// its placeholder runs, so its lines break and align at the width the
/// numbers take.
pub(crate) fn fill_numbers(para: &Para<'_>, numbers: PageNumbers) -> Prepared {
    let Para {
        prepared: p,
        runs,
        fonts,
        ..
    } = *para;
    let mut glyphs = Vec::with_capacity(p.glyphs.len() + 4);
    let mut segments: Vec<Segment> = Vec::with_capacity(p.segments.len());
    // Per segment: (first old glyph, first new glyph) for remapping breaks.
    let mut starts: Vec<(u32, u32)> = Vec::with_capacity(p.segments.len());
    #[cfg(test)]
    let mut text = String::with_capacity(p.text.len() + 4);
    #[cfg(test)]
    let mut copied = 0usize;
    // Bytes the text has grown by so far.
    let mut grown = 0i64;
    let shift = |at: u32, grown: i64| (i64::from(at) + grown) as u32;
    for seg in &p.segments {
        let run = &runs[seg.run as usize];
        let first = glyphs.len() as u32;
        let text_start = shift(seg.text.start, grown);
        starts.push((seg.glyphs.start, first));
        let Some(ph) = run.placeholder else {
            glyphs.extend(
                p.glyphs[seg.glyphs.start as usize..seg.glyphs.end as usize]
                    .iter()
                    .map(|g| Glyph {
                        cluster: shift(g.cluster, grown),
                        ..*g
                    }),
            );
            segments.push(Segment {
                text: text_start..shift(seg.text.end, grown),
                glyphs: first..glyphs.len() as u32,
                ..seg.clone()
            });
            continue;
        };
        // A placeholder is one segment (its digits share a font and a
        // level); any later piece of it is dropped with its stand-ins.
        if segments.last().is_some_and(|s| s.run == seg.run) {
            grown -= i64::from(seg.text.end - seg.text.start);
            #[cfg(test)]
            {
                copied = seg.text.end as usize;
            }
            continue;
        }
        let n = numbers.of(ph);
        let digits = n.to_string();
        let at = glyphs.len();
        for (i, (id, advance)) in number_glyphs(fonts.get(seg.font), run, n)
            .into_iter()
            .enumerate()
        {
            glyphs.push(Glyph {
                id,
                advance,
                x_offset: 0.0,
                y_offset: 0.0,
                cluster: text_start + i as u32,
                flags: 0,
            });
        }
        // Painting reverses right-to-left segments; numbers read left to
        // right whatever the level.
        if seg.level % 2 == 1 {
            glyphs[at..].reverse();
        }
        #[cfg(test)]
        {
            text.push_str(&p.text[copied..seg.text.start as usize]);
            text.push_str(&digits);
            copied = seg.text.end as usize;
        }
        grown += digits.len() as i64 - i64::from(seg.text.end - seg.text.start);
        segments.push(Segment {
            text: text_start..text_start + digits.len() as u32,
            glyphs: first..glyphs.len() as u32,
            ..seg.clone()
        });
    }
    #[cfg(test)]
    text.push_str(&p.text[copied..]);

    // No break falls inside a number (UAX #14 keeps digits together), so
    // every break moves with the segment it starts.
    let breaks = p
        .breaks
        .iter()
        .map(|b| {
            let k = starts.partition_point(|&(old, _)| old <= b.glyph).max(1) - 1;
            let (old, new) = starts.get(k).copied().unwrap_or((0, 0));
            BreakPoint {
                glyph: new + (b.glyph - old),
                kind: b.kind,
            }
        })
        .collect::<Vec<_>>();
    // The numbers move the tab stops after them.
    place_tabs(&mut glyphs, &segments, &breaks, runs, fonts);

    Prepared {
        #[cfg(test)]
        text,
        segments,
        glyphs,
        breaks,
        rtl: p.rtl,
        bidi: p.bidi,
        placeholders: false,
        objects: p.objects.clone(),
    }
}

/// Bidi embedding levels per byte of `text`, or `None` when the text is
/// left to right throughout (the common case, skipped cheaply). Also the
/// resolved base direction.
fn bidi_levels(text: &str, direction: Direction) -> (Option<Vec<u8>>, bool) {
    // Hebrew (U+0590) and later blocks hold every strong right-to-left
    // character; explicit marks and embeddings start at U+200E.
    let maybe_rtl = text.chars().any(|c| c >= '\u{0590}');
    if direction == Direction::Ltr && !maybe_rtl {
        return (None, false);
    }
    let level = match direction {
        Direction::Ltr => Some(unicode_bidi::Level::ltr()),
        Direction::Rtl => Some(unicode_bidi::Level::rtl()),
        Direction::Auto => None,
    };
    let info = unicode_bidi::BidiInfo::new(text, level);
    let rtl = info.paragraphs.first().is_some_and(|p| p.level.is_rtl())
        || (text.is_empty() && direction == Direction::Rtl);
    let levels: Vec<u8> = info
        .levels
        .iter()
        .map(unicode_bidi::Level::number)
        .collect();
    if !rtl && levels.iter().all(|l| *l == 0) {
        return (None, false);
    }
    (Some(levels), rtl)
}

#[cfg(test)]
mod tests {
    use super::super::hyphen::{HypherHyphenator, NoHyphenation};
    use super::*;

    fn run(text: &str) -> TextRun {
        TextRun {
            image: None,
            text: Some(text.into()),
            font: "f".into(),
            size: 10.0,
            color: None,
            letter_spacing: None,
            decoration: None,
            decoration_color: None,
            decoration_thickness: None,
            decoration_style: None,
            placeholder: None,
            link: None,
            hyphenate: None,
            vertical_align: None,
            font_features: None,
        }
    }

    fn text_of(runs: &[TextRun], ws: WhiteSpace) -> String {
        build_text(&ParagraphOptions {
            white_space: ws,
            direction: Direction::Ltr,
            hyphenate: false,
            lang: None,
            hyphenator: &NoHyphenation,
            objects: &super::super::InlineObjects::default(),
            runs,
        })
        .0
    }

    #[test]
    fn collapses_whitespace_across_runs() {
        let runs = [run("  Hello \t "), run("  world  "), run("!")];
        assert_eq!(text_of(&runs, WhiteSpace::Normal), "Hello world !");
        // The collapsed space stays in the run where it occurs.
        let (text, ranges) = build_text(&ParagraphOptions {
            white_space: WhiteSpace::Normal,
            direction: Direction::Ltr,
            hyphenate: false,
            lang: None,
            hyphenator: &NoHyphenation,
            objects: &super::super::InlineObjects::default(),
            runs: &[run("a "), run(" b")],
        });
        assert_eq!(text, "a b");
        assert_eq!(ranges, vec![(0, 2), (2, 3)]);
    }

    #[test]
    fn newlines_force_breaks_and_eat_spaces() {
        let runs = [run("a  \n  b")];
        assert_eq!(text_of(&runs, WhiteSpace::Normal), "a\nb");
    }

    #[test]
    fn pre_keeps_spaces() {
        let runs = [run(" a\tb\r\n")];
        assert_eq!(text_of(&runs, WhiteSpace::Pre), " a\tb\n");
    }

    #[test]
    fn pre_wrap_keeps_spaces_and_pre_line_collapses() {
        let runs = [run("a  b\tc\n d")];
        assert_eq!(text_of(&runs, WhiteSpace::PreWrap), "a  b\tc\n d");
        assert_eq!(text_of(&runs, WhiteSpace::PreLine), "a b c\nd");
    }

    #[test]
    fn only_spaces_tabs_and_breaks_collapse() {
        // Thin, em, figure and ideographic spaces keep their own width.
        let runs = [run("A\u{2009}B\u{2003}C\u{2007}D\u{3000}E \t\r F\u{000C}G")];
        assert_eq!(
            text_of(&runs, WhiteSpace::Normal),
            "A\u{2009}B\u{2003}C\u{2007}D\u{3000}E F G"
        );
    }

    #[test]
    fn bidi_levels_only_when_needed() {
        assert_eq!(bidi_levels("plain text", Direction::Ltr), (None, false));
        let (levels, rtl) = bidi_levels("ab \u{05E9}\u{05DC}", Direction::Ltr);
        assert!(!rtl);
        assert_eq!(levels.unwrap().last(), Some(&1));
        let (_, rtl) = bidi_levels("\u{05E9}\u{05DC} ab", Direction::Auto);
        assert!(rtl);
        let (levels, rtl) = bidi_levels("ab", Direction::Rtl);
        assert!(rtl);
        assert_eq!(levels.unwrap(), vec![2, 2]);
    }

    #[test]
    fn nbsp_is_not_collapsed() {
        let runs = [run("1\u{00A0}000")];
        assert_eq!(text_of(&runs, WhiteSpace::Normal), "1\u{00A0}000");
    }

    #[test]
    fn soft_hyphens_in_long_words_only() {
        let out = insert_soft_hyphens("a documentation hyphenation", &HypherHyphenator, "en");
        assert!(out.starts_with("a "));
        assert!(
            out.contains("doc\u{AD}u\u{AD}men\u{AD}ta\u{AD}tion"),
            "{out:?}"
        );
        assert_eq!(out.replace('\u{AD}', ""), "a documentation hyphenation");
    }

    #[test]
    fn a_run_overrides_the_paragraph_hyphenation() {
        let text = |hyphenate: bool, runs: &[TextRun]| {
            build_text(&ParagraphOptions {
                white_space: WhiteSpace::Normal,
                direction: Direction::Ltr,
                hyphenate,
                lang: Some("en"),
                hyphenator: &HypherHyphenator,
                objects: &super::super::InlineObjects::default(),
                runs,
            })
            .0
        };
        let mut kept = run("documentation");
        kept.hyphenate = Some(false);
        let runs = [run("documentation "), kept];
        assert_eq!(
            text(true, &runs),
            "doc\u{AD}u\u{AD}men\u{AD}ta\u{AD}tion documentation"
        );
        let mut on = run("documentation");
        on.hyphenate = Some(true);
        let runs = [run("documentation "), on];
        assert_eq!(
            text(false, &runs),
            "documentation doc\u{AD}u\u{AD}men\u{AD}ta\u{AD}tion"
        );
    }

    #[test]
    fn built_in_languages() {
        assert!(HypherHyphenator.supports("de"));
        // Languages without built-in patterns.
        assert!(!HypherHyphenator.supports("ja"));
    }
}
