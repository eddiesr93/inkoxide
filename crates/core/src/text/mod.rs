//! Text: shaping (rustybuzz), line breaking (UAX #14), hyphenation
//! (hypher), and the per-render paragraph cache.
//!
//! Nothing outside this module sees shaping-library types: the rest of the
//! engine works with [`Prepared`] paragraphs and [`LineLayout`]s. Inside
//! it, only `prepare.rs` uses rustybuzz; it builds its shaping faces from
//! the font bytes, and takes glyph lookups and advances from `fonts`.

mod hyphen;
pub use hyphen::HyphenationStore;
pub(crate) use hyphen::ResolvedHyphenator;
mod lines;
mod optimal;
mod paint;
mod prepare;
mod svg;
pub(crate) use prepare::{ShapingCause, ShapingFailure, MAX_EXTRA_GLYPHS};

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

use rayon::prelude::*;

pub(crate) use paint::{PageNumbers, TextPainter};
#[cfg(test)]
pub(crate) use svg::SvgGlyph;
pub(crate) use svg::{lay_out as lay_out_svg_text, LaidText};

use crate::fonts::{FontKey, FontSet};
use crate::layout::{AvailableSpace, LeafMeasure, LineBoxes, Size};
use crate::schema::{
    Direction, Node, NodeKind, RunAlign, Style, TextAlign, TextRun, TextWrap, WhiteSpace,
};
use crate::InkoxideError;

const GLYPH_SPACE: u8 = 1;
const GLYPH_INVISIBLE: u8 = 2;
const GLYPH_TAB: u8 = 4;

/// Tolerance for float noise when comparing widths.
const WIDTH_EPS: f32 = 1e-3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Glyph {
    pub id: u16,
    /// Advance in points, letter spacing included.
    pub advance: f32,
    pub x_offset: f32,
    /// Points, positive up (font convention).
    pub y_offset: f32,
    /// Byte offset of the glyph's cluster in [`Prepared::text`].
    pub cluster: u32,
    pub flags: u8,
}

impl Glyph {
    /// A space (justification stretches it).
    pub fn is_space(&self) -> bool {
        self.flags & GLYPH_SPACE != 0
    }

    /// Drawn with no ink (a newline, a soft hyphen, a control character).
    pub fn is_invisible(&self) -> bool {
        self.flags & GLYPH_INVISIBLE != 0
    }

    /// A tab: invisible, as wide as the way to the next tab stop.
    pub fn is_tab(&self) -> bool {
        self.flags & GLYPH_TAB != 0
    }
}

/// A stretch of text in one run and one font.
#[derive(Debug, Clone)]
pub(crate) struct Segment {
    pub run: u16,
    pub font: FontKey,
    /// Bidi embedding level of the whole segment.
    pub level: u8,
    pub text: Range<u32>,
    pub glyphs: Range<u32>,
}

impl Segment {
    fn new(run: usize, font: FontKey, level: u8, text: Range<usize>) -> Self {
        Self {
            run: run as u16,
            font,
            level,
            text: text.start as u32..text.end as u32,
            glyphs: 0..0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BreakKind {
    Allowed,
    Mandatory,
    /// At a soft hyphen: shows a hyphen when taken.
    Hyphen,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BreakPoint {
    /// Index of the first glyph of the next line.
    pub glyph: u32,
    pub kind: BreakKind,
}

/// A shaped paragraph, independent of the available width.
#[derive(Debug, Default)]
pub(crate) struct Prepared {
    /// The paragraph text that glyph clusters index into (tests map lines
    /// back to text with it).
    #[cfg(test)]
    pub text: String,
    pub segments: Vec<Segment>,
    pub glyphs: Vec<Glyph>,
    pub breaks: Vec<BreakPoint>,
    /// Resolved base direction is right to left.
    pub rtl: bool,
    /// Some glyph has an odd bidi level (lines need reordering).
    pub bidi: bool,
    /// Placeholder runs hold digits to lay out (the stand-ins of
    /// [`prepare::PLACEHOLDER_TEXT`], or the numbers of the page flow text
    /// lands on); the painter draws the page's own numbers in their place.
    pub placeholders: bool,
}

impl Prepared {
    /// The end of glyphs `start..end` without their trailing spaces and
    /// invisible glyphs.
    pub fn visible_end(&self, start: usize, end: usize) -> usize {
        let mut e = end;
        while e > start && (self.glyphs[e - 1].is_space() || self.glyphs[e - 1].is_invisible()) {
            e -= 1;
        }
        e
    }

    pub fn segment_of(&self, glyph: usize) -> Option<&Segment> {
        let g = glyph as u32;
        let i = self.segments.partition_point(|s| s.glyphs.end <= g);
        self.segments
            .get(i)
            .filter(|s| s.glyphs.start <= g && g < s.glyphs.end)
    }
}

/// An extra glyph appended to a line (hyphen or ellipsis).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtraGlyph {
    pub font: FontKey,
    pub run: u16,
    pub glyph: u16,
    /// How many times the glyph repeats (3 for a `...` fallback).
    pub count: u8,
    pub advance: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct Line {
    /// Visible glyphs `start..end` of the paragraph (trailing spaces cut).
    pub start: u32,
    pub end: u32,
    /// Natural width including any hyphen / ellipsis.
    pub width: f32,
    pub hyphen: Option<ExtraGlyph>,
    pub ellipsis: Option<ExtraGlyph>,
    /// Ends at a forced break or the end of the paragraph (not justified).
    pub forced: bool,
    /// `textIndent` of the first line (0 on the others).
    pub indent: f32,
    /// Offset of the line box from the top of the paragraph.
    pub top: f32,
    /// Baseline offset from the top of the line box.
    pub ascent: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LineLayout {
    pub lines: Vec<Line>,
    /// Widest line, from the left of the content box.
    pub width: f32,
    pub height: f32,
    /// Each line's box, its left edge and width, when the boxes the lines
    /// were broken in have bands; empty when every line is as wide as the
    /// content box.
    pub boxes: Vec<(f32, f32)>,
}

/// Paragraph-level settings resolved from the node style.
#[derive(Debug, Clone)]
pub(crate) struct ParaStyle {
    pub align: TextAlign,
    pub line_height: Option<f32>,
    pub max_lines: Option<u32>,
    pub ellipsis: bool,
    pub white_space: WhiteSpace,
    pub text_wrap: TextWrap,
    pub indent: f32,
    pub direction: Direction,
    /// Whether the paragraph's words are hyphenated; a run's own
    /// `hyphenate` overrides it.
    pub hyphenate: bool,
    /// The paragraph's language tag, normalised (`en-gb`), for hyphenation
    /// (patterns may still be missing; the hyphenator decides).
    pub lang: Option<String>,
}

impl ParaStyle {
    /// Whether lines wrap at soft break opportunities.
    pub fn wrap(&self) -> bool {
        !matches!(self.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre)
    }

    pub fn from_style(s: &Style, doc_lang: Option<&str>) -> Self {
        let white_space = s.white_space.unwrap_or(WhiteSpace::Normal);
        Self {
            align: s.text_align.unwrap_or(TextAlign::Start),
            line_height: s.line_height,
            max_lines: s.max_lines,
            ellipsis: s.ellipsis.unwrap_or(false),
            white_space,
            text_wrap: s.text_wrap.unwrap_or(TextWrap::Wrap),
            indent: s.text_indent.unwrap_or(0.0),
            direction: s.direction.unwrap_or(Direction::Ltr),
            hyphenate: s.hyphenate == Some(true),
            lang: s
                .lang
                .as_deref()
                .or(doc_lang)
                .and_then(hyphen::paragraph_tag),
        }
    }
}

/// A shaped paragraph with what its lines are broken and measured with.
#[derive(Clone, Copy)]
pub(crate) struct Para<'p> {
    pub prepared: &'p Prepared,
    pub runs: &'p [TextRun],
    pub style: &'p ParaStyle,
    pub fonts: &'p FontSet<'p>,
}

/// How far `run`'s baseline sits above the line's, in points (negative:
/// below): its `verticalAlign`, by its font's superscript or subscript
/// offset for text of its size.
pub(crate) fn baseline_shift(run: &TextRun, fonts: &FontSet<'_>) -> f32 {
    // Validation checks that the run's font is declared.
    let Some(key) = fonts.key(&run.font) else {
        return 0.0;
    };
    let m = &fonts.get(key).data.metrics;
    match run.vertical_align.unwrap_or_default() {
        RunAlign::Baseline => 0.0,
        RunAlign::Super => m.superscript_shift() * run.size,
        RunAlign::Sub => -m.subscript_shift() * run.size,
    }
}

/// Glyph → text mappings per font, for the PDF `ToUnicode` `CMaps`.
#[derive(Debug, Default)]
pub(crate) struct UnicodeMap {
    pub fonts: BTreeMap<FontKey, BTreeMap<u16, String>>,
}

impl UnicodeMap {
    pub fn record(&mut self, font: FontKey, gid: u16, text: &str) {
        self.fonts
            .entry(font)
            .or_default()
            .entry(gid)
            .or_insert_with(|| text.to_string());
    }

    /// Adds `other`'s mappings, keeping the first mapping of each glyph.
    pub fn merge_first(&mut self, other: UnicodeMap) {
        for (font, map) in other.fonts {
            let target = self.fonts.entry(font).or_default();
            for (gid, text) in map {
                target.entry(gid).or_insert(text);
            }
        }
    }
}

/// Dependency firewall around text shaping. Line breaking and page-number
/// filling work on its [`Prepared`] paragraphs, with its fonts.
pub(crate) trait TextShaper {
    /// Shapes a paragraph once, independently of width.
    fn prepare(&mut self, runs: &[TextRun], style: &ParaStyle) -> Prepared;
    /// The fonts it shapes with.
    fn fonts(&self) -> &FontSet<'_>;
}

/// rustybuzz + unicode-linebreak + hypher implementation.
pub(crate) struct RustybuzzShaper<'f, 'a> {
    fonts: &'f FontSet<'a>,
    unicode: UnicodeMap,
    hyphenator: &'f dyn hyphen::Hyphenator,
    shapes: prepare::ShapeCache<'a>,
}

impl<'f, 'a> RustybuzzShaper<'f, 'a> {
    /// Parses the fonts' shaping faces, once per render: shapers for
    /// other threads share them ([`TextCache::prepare_all`]). Hyphenates
    /// with hypher's built-in patterns, unless given a
    /// [hyphenator](Self::with_hyphenator).
    pub fn new(fonts: &'f FontSet<'a>) -> Result<Self, InkoxideError> {
        let faces = prepare::Faces::load(fonts)?;
        Ok(Self::with_shapes(fonts, prepare::ShapeCache::new(faces)))
    }

    /// This shaper, hyphenating with `hyphenator`.
    pub fn with_hyphenator(self, hyphenator: &'f dyn hyphen::Hyphenator) -> Self {
        Self { hyphenator, ..self }
    }

    /// A shaper with the same settings and shaping faces, and glyph
    /// mappings of its own, for another thread.
    fn worker(&self) -> Self {
        Self::with_shapes(self.fonts, self.shapes.sibling()).with_hyphenator(self.hyphenator)
    }

    fn with_shapes(fonts: &'f FontSet<'a>, shapes: prepare::ShapeCache<'a>) -> Self {
        let mut unicode = UnicodeMap::default();
        // Glyphs the painter adds without shaping: hyphens, ellipses and
        // placeholder digits.
        for key in 0..fonts.fonts.len() as FontKey {
            let f = fonts.get(key);
            let extras = [lines::HYPHEN, '-', lines::ELLIPSIS, '.'];
            for c in extras.into_iter().chain('0'..='9') {
                if let Some(g) = f.glyph(c) {
                    let mut buf = [0; 4];
                    let text = if c == lines::HYPHEN {
                        "-"
                    } else {
                        c.encode_utf8(&mut buf)
                    };
                    unicode.record(key, g, text);
                }
            }
        }
        Self {
            fonts,
            unicode,
            hyphenator: &hyphen::HypherHyphenator,
            shapes,
        }
    }
}

impl TextShaper for RustybuzzShaper<'_, '_> {
    fn prepare(&mut self, runs: &[TextRun], style: &ParaStyle) -> Prepared {
        let lang = style
            .lang
            .as_deref()
            .filter(|l| self.hyphenator.supports(l));
        let opts = prepare::ParagraphOptions {
            white_space: style.white_space,
            direction: style.direction,
            hyphenate: style.hyphenate,
            lang,
            hyphenator: self.hyphenator,
            runs,
        };
        prepare::prepare(&opts, self.fonts, &mut self.shapes, &mut self.unicode)
    }

    fn fonts(&self) -> &FontSet<'_> {
        self.fonts
    }
}

struct Entry {
    style: ParaStyle,
    prepared: Prepared,
    /// Widest unbreakable piece (min-content width).
    min_content: f32,
    /// Line layouts by the boxes they were broken in.
    layouts: Vec<(LineBoxes, Arc<LineLayout>)>,
}

impl Entry {
    fn new(fonts: &FontSet<'_>, runs: &[TextRun], style: ParaStyle, prepared: Prepared) -> Self {
        let para = Para {
            prepared: &prepared,
            runs,
            style: &style,
            fonts,
        };
        Self {
            min_content: lines::min_content(&para),
            style,
            prepared,
            layouts: Vec::new(),
        }
    }

    /// Shapes a text node into a cache entry.
    fn shape<S: TextShaper>(
        shaper: &mut S,
        node: &Node,
        runs: &[TextRun],
        lang: Option<&str>,
    ) -> Self {
        let style = ParaStyle::from_style(&node.style, lang);
        let prepared = shaper.prepare(runs, &style);
        Self::new(shaper.fonts(), runs, style, prepared)
    }

    /// The entry with `numbers` in place of its placeholders' stand-in
    /// digits.
    fn with_numbers(&self, fonts: &FontSet<'_>, runs: &[TextRun], numbers: PageNumbers) -> Self {
        let prepared = prepare::fill_numbers(&self.para(fonts, runs), numbers);
        Self::new(fonts, runs, self.style.clone(), prepared)
    }

    fn para<'p>(&'p self, fonts: &'p FontSet<'_>, runs: &'p [TextRun]) -> Para<'p> {
        Para {
            prepared: &self.prepared,
            runs,
            style: &self.style,
            fonts,
        }
    }

    fn cached(&self, boxes: &LineBoxes) -> Option<Arc<LineLayout>> {
        self.layouts
            .iter()
            .find(|(k, _)| k.same(boxes))
            .map(|(_, l)| l.clone())
    }

    fn break_lines(&self, fonts: &FontSet<'_>, runs: &[TextRun], boxes: &LineBoxes) -> LineLayout {
        let boxes = LineBoxes {
            width: boxes.width.max(0.0),
            ..boxes.clone()
        };
        lines::break_lines(&self.para(fonts, runs), &boxes)
    }

    /// The lines in `boxes`, cached.
    fn layout(
        &mut self,
        fonts: &FontSet<'_>,
        runs: &[TextRun],
        boxes: &LineBoxes,
    ) -> Arc<LineLayout> {
        if let Some(l) = self.cached(boxes) {
            return l;
        }
        let layout = Arc::new(self.break_lines(fonts, runs, boxes));
        self.layouts.push((boxes.clone(), layout.clone()));
        layout
    }

    /// The lines in `boxes`: cached, or computed without caching.
    fn layout_at(
        &self,
        fonts: &FontSet<'_>,
        runs: &[TextRun],
        boxes: &LineBoxes,
    ) -> Arc<LineLayout> {
        self.cached(boxes)
            .unwrap_or_else(|| Arc::new(self.break_lines(fonts, runs, boxes)))
    }

    /// [`layout_at`](Self::layout_at), with the text and style the lines
    /// were broken from.
    fn paragraph(
        &self,
        fonts: &FontSet<'_>,
        runs: &[TextRun],
        boxes: &LineBoxes,
    ) -> (Arc<LineLayout>, &Prepared, &ParaStyle) {
        (
            self.layout_at(fonts, runs, boxes),
            &self.prepared,
            &self.style,
        )
    }

    /// The width to break lines at in the available space.
    fn width_in(&self, available: AvailableSpace) -> f32 {
        match available {
            AvailableSpace::Definite(w) => w,
            AvailableSpace::MaxContent => f32::INFINITY,
            // Truncating text can shrink to (almost) nothing.
            AvailableSpace::MinContent if self.style.ellipsis => 0.0,
            AvailableSpace::MinContent => self.min_content,
        }
    }

    /// The paragraph's size as a layout leaf, its lines cached.
    fn measure(
        &mut self,
        fonts: &FontSet<'_>,
        runs: &[TextRun],
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = self.width_in(available.width);
        leaf_size(
            &self.layout(fonts, runs, &LineBoxes::plain(width)),
            known,
            width,
        )
    }

    /// [`measure`](Self::measure) for an entry only read: its lines are
    /// not cached.
    fn measure_at(
        &self,
        fonts: &FontSet<'_>,
        runs: &[TextRun],
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = self.width_in(available.width);
        leaf_size(
            &self.layout_at(fonts, runs, &LineBoxes::plain(width)),
            known,
            width,
        )
    }
}

/// A text leaf's size from its lines at `width`. The layout engine's
/// `known` is the border box; the width it makes available is already the
/// content box (the known width minus padding and border).
fn leaf_size(l: &LineLayout, known: Size<Option<f32>>, width: f32) -> Size<f32> {
    Size {
        width: known.width.unwrap_or(l.width.min(width.max(0.0))),
        height: known.height.unwrap_or(l.height),
    }
}

/// Per-render cache of shaped paragraphs, keyed by node identity. Also
/// the layout engine's measure function for text leaves.
pub(crate) struct TextCache<'s, S: TextShaper> {
    shaper: S,
    doc_lang: Option<&'s str>,
    entries: HashMap<usize, Entry>,
    /// Flow paragraphs laid out with page numbers ([`TextCache::set_numbers`]):
    /// their entries with the stand-in digits, and the numbers in place of them.
    numbered: HashMap<usize, (Entry, PageNumbers)>,
}

fn node_key(n: &Node) -> usize {
    std::ptr::from_ref::<Node>(n) as usize
}

impl TextCache<'_, RustybuzzShaper<'_, '_>> {
    /// Shapes `nodes` (text nodes in document order) ahead of layout, in
    /// parallel on the current rayon pool unless `parallel` is false. Each
    /// worker has its own shaper; glyph-to-Unicode maps are merged in
    /// document order, keeping the first mapping of each glyph, so the
    /// result does not depend on the thread count.
    pub fn prepare_all(&mut self, nodes: &[&Node], parallel: bool) {
        const MIN_CHUNK: usize = 64;
        let todo: Vec<&Node> = nodes
            .iter()
            .copied()
            .filter(|n| matches!(n.kind, NodeKind::Text { .. }))
            .filter(|n| !self.entries.contains_key(&node_key(n)))
            .collect();
        let threads = if parallel {
            rayon::current_num_threads()
        } else {
            1
        };
        if threads <= 1 || todo.len() < 2 * MIN_CHUNK {
            for n in todo {
                if let NodeKind::Text { runs } = &n.kind {
                    self.entry(n, runs);
                }
            }
            return;
        }
        let chunk = todo.len().div_ceil(threads * 4).max(MIN_CHUNK);
        let (shaper, lang) = (&self.shaper, self.doc_lang);
        let parts: Vec<(Vec<(usize, Entry)>, UnicodeMap)> = todo
            .par_chunks(chunk)
            .map(|nodes| {
                let mut shaper = shaper.worker();
                let entries = nodes
                    .iter()
                    .filter_map(|n| match &n.kind {
                        NodeKind::Text { runs } => {
                            Some((node_key(n), Entry::shape(&mut shaper, n, runs, lang)))
                        }
                        _ => None,
                    })
                    .collect();
                (entries, shaper.unicode)
            })
            .collect();
        for (entries, unicode) in parts {
            self.entries.extend(entries);
            self.shaper.unicode.merge_first(unicode);
        }
    }

    /// The glyph → text mappings of every paragraph shaped, for the PDF.
    pub fn into_unicode(self) -> UnicodeMap {
        self.shaper.unicode
    }
}

impl<'s, S: TextShaper> TextCache<'s, S> {
    pub fn new(shaper: S, doc_lang: Option<&'s str>) -> Self {
        Self {
            shaper,
            doc_lang,
            entries: HashMap::new(),
            numbered: HashMap::new(),
        }
    }

    /// First glyph of a page-number placeholder in text node `node`, if it
    /// shows one.
    pub fn placeholder_glyph(&self, node: &Node) -> Option<u32> {
        let NodeKind::Text { runs } = &node.kind else {
            return None;
        };
        let e = self.entries.get(&node_key(node))?;
        e.prepared
            .segments
            .iter()
            .find(|s| runs[s.run as usize].placeholder.is_some())
            .map(|s| s.glyphs.start)
    }

    /// Lays out flow paragraph `node` with `numbers` in place of its
    /// placeholders' stand-in digits, so its lines break, align and size at
    /// the width they take. Returns whether its numbers changed.
    pub fn set_numbers(&mut self, node: &Node, numbers: PageNumbers) -> bool {
        let NodeKind::Text { runs } = &node.kind else {
            return false;
        };
        let key = node_key(node);
        if self.numbered.get(&key).is_some_and(|(_, n)| *n == numbers) {
            return false;
        }
        let Some(current) = self.entries.remove(&key) else {
            return false;
        };
        let (stand_in, used) = self.numbered.entry(key).or_insert((current, numbers));
        *used = numbers;
        let mut entry = stand_in.with_numbers(self.shaper.fonts(), runs, numbers);
        // The painter still draws the page's own digits: the same glyphs,
        // unless the paragraph ended up on another page than it was laid
        // out for.
        entry.prepared.placeholders = true;
        self.entries.insert(key, entry);
        true
    }

    /// The entry of text node `node`, shaped if it is not yet, and the
    /// fonts to lay it out with.
    fn entry(&mut self, node: &Node, runs: &[TextRun]) -> (&mut Entry, &FontSet<'_>) {
        let lang = self.doc_lang;
        let shaper = &mut self.shaper;
        let entry = self
            .entries
            .entry(node_key(node))
            .or_insert_with(|| Entry::shape(shaper, node, runs, lang));
        (entry, self.shaper.fonts())
    }

    /// The shaper, for text laid out outside paragraphs (SVG text).
    pub fn shaper(&mut self) -> &mut S {
        &mut self.shaper
    }

    pub fn layout(&mut self, node: &Node, width: f32) -> Option<Arc<LineLayout>> {
        self.layout_in(node, &LineBoxes::plain(width))
    }

    /// The lines of text node `node` in `boxes`, cached.
    pub fn layout_in(&mut self, node: &Node, boxes: &LineBoxes) -> Option<Arc<LineLayout>> {
        let NodeKind::Text { runs } = &node.kind else {
            return None;
        };
        let (entry, fonts) = self.entry(node, runs);
        Some(entry.layout(fonts, runs, boxes))
    }

    /// Read-only [`layout`](Self::layout) for painting (which runs in
    /// parallel): the cached layout at `width`, or one computed without
    /// caching, with the text and style it was broken from. Layout has
    /// already measured every painted text at its final width, so the
    /// fallback is rare.
    pub fn paragraph(
        &self,
        node: &Node,
        width: f32,
    ) -> Option<(Arc<LineLayout>, &Prepared, &ParaStyle)> {
        let NodeKind::Text { runs } = &node.kind else {
            return None;
        };
        let e = self.entries.get(&node_key(node))?;
        Some(e.paragraph(self.shaper.fonts(), runs, &LineBoxes::plain(width)))
    }

    /// How many glyphs text node `node` shaped to, once it is shaped.
    pub fn glyph_count(&self, node: &Node) -> Option<usize> {
        let e = self.entries.get(&node_key(node))?;
        Some(e.prepared.glyphs.len())
    }

    #[cfg(test)]
    pub fn prepared(&self, node: &Node) -> Option<(&Prepared, &ParaStyle)> {
        self.entries
            .get(&node_key(node))
            .map(|e| (&e.prepared, &e.style))
    }
}

impl<S: TextShaper> LeafMeasure for TextCache<'_, S> {
    fn measure(
        &mut self,
        node: &Node,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let NodeKind::Text { runs } = &node.kind else {
            return Size::ZERO;
        };
        let (entry, fonts) = self.entry(node, runs);
        entry.measure(fonts, runs, known, available)
    }
}

/// The text of one page's headers, footers and overlays, for laying them
/// out again with that page's numbers. Paragraphs with placeholders get the
/// numbers in place of their stand-in digits, and line layouts of their
/// own; the rest comes from the shared cache, which pages only read, so
/// they can be laid out in parallel.
pub(crate) struct PageText<'c, 's, S: TextShaper> {
    cache: &'c TextCache<'s, S>,
    numbers: PageNumbers,
    /// Paragraphs with placeholders, by node.
    entries: HashMap<usize, Entry>,
}

impl<'c, 's, S: TextShaper> PageText<'c, 's, S> {
    pub fn new(cache: &'c TextCache<'s, S>, numbers: PageNumbers) -> Self {
        Self {
            cache,
            numbers,
            entries: HashMap::new(),
        }
    }

    /// For a paragraph with placeholders measured on this page: its lines
    /// at `width`, and the text and style they were broken from.
    pub fn paragraph(
        &self,
        node: &Node,
        width: f32,
    ) -> Option<(Arc<LineLayout>, &Prepared, &ParaStyle)> {
        let NodeKind::Text { runs } = &node.kind else {
            return None;
        };
        let e = self.entries.get(&node_key(node))?;
        Some(e.paragraph(self.cache.shaper.fonts(), runs, &LineBoxes::plain(width)))
    }
}

impl<S: TextShaper> LeafMeasure for PageText<'_, '_, S> {
    fn measure(
        &mut self,
        node: &Node,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let NodeKind::Text { runs } = &node.kind else {
            return Size::ZERO;
        };
        let key = node_key(node);
        // Every header, footer and overlay paragraph was shaped before
        // layout.
        let Some(shared) = self.cache.entries.get(&key) else {
            return Size::ZERO;
        };
        let fonts = self.cache.shaper.fonts();
        if !shared.prepared.placeholders {
            return shared.measure_at(fonts, runs, known, available);
        }
        let numbers = self.numbers;
        self.entries
            .entry(key)
            .or_insert_with(|| shared.with_numbers(fonts, runs, numbers))
            .measure(fonts, runs, known, available)
    }
}

#[cfg(test)]
mod tests;
