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
mod paint;
mod prepare;

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

use rayon::prelude::*;

pub(crate) use paint::{PageNumbers, TextPainter};

use crate::fonts::{FontKey, FontSet};
use crate::layout::{width_key, AvailableSpace, LeafMeasure, Size};
use crate::schema::{Direction, Node, NodeKind, Style, TextAlign, TextRun, WhiteSpace};
use crate::InkoxideError;

pub(crate) const GLYPH_SPACE: u8 = 1;
pub(crate) const GLYPH_INVISIBLE: u8 = 2;

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
    /// Widest line.
    pub width: f32,
    pub height: f32,
}

/// Paragraph-level settings resolved from the node style.
#[derive(Debug, Clone)]
pub(crate) struct ParaStyle {
    pub align: TextAlign,
    pub line_height: Option<f32>,
    pub max_lines: Option<u32>,
    pub ellipsis: bool,
    pub white_space: WhiteSpace,
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
}

/// Dependency firewall around text shaping.
pub(crate) trait TextShaper {
    /// Shapes a paragraph once, independently of width.
    fn prepare(&mut self, runs: &[TextRun], style: &ParaStyle) -> Prepared;
    /// Breaks a prepared paragraph into lines at `max_width`.
    fn break_lines(
        &self,
        p: &Prepared,
        runs: &[TextRun],
        style: &ParaStyle,
        max_width: f32,
    ) -> LineLayout;
    /// `p` with the page numbers in place of its placeholders' stand-in
    /// digits.
    fn fill_numbers(&self, p: &Prepared, runs: &[TextRun], numbers: PageNumbers) -> Prepared;
    /// The widest unbreakable piece of a prepared paragraph.
    fn min_content(&self, p: &Prepared, runs: &[TextRun], style: &ParaStyle) -> f32;
}

/// rustybuzz + unicode-linebreak + hypher implementation.
pub(crate) struct RustybuzzShaper<'f, 'a> {
    pub fonts: &'f FontSet<'a>,
    pub unicode: UnicodeMap,
    pub hyphenator: &'f dyn hyphen::Hyphenator,
    shapes: prepare::ShapeCache<'a>,
}

impl<'f, 'a> RustybuzzShaper<'f, 'a> {
    /// Parses the fonts' shaping faces, once per render: shapers for
    /// other threads share them ([`TextCache::prepare_all`]).
    pub fn new(fonts: &'f FontSet<'a>) -> Result<Self, InkoxideError> {
        Ok(Self::with_faces(fonts, prepare::Faces::load(fonts)?))
    }

    fn with_faces(fonts: &'f FontSet<'a>, faces: prepare::Faces<'a>) -> Self {
        let mut unicode = UnicodeMap::default();
        // Glyphs the painter adds without shaping: hyphens, ellipses and
        // placeholder digits.
        for key in 0..fonts.fonts.len() as FontKey {
            let f = fonts.get(key);
            for c in [
                '\u{2010}', '-', '\u{2026}', '.', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
            ] {
                if let Some(g) = f.glyph(c) {
                    let mut buf = [0; 4];
                    let text = if c == '\u{2010}' {
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
            shapes: prepare::ShapeCache::new(faces),
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

    fn break_lines(
        &self,
        p: &Prepared,
        runs: &[TextRun],
        style: &ParaStyle,
        max_width: f32,
    ) -> LineLayout {
        lines::break_lines(p, runs, style, self.fonts, max_width)
    }

    fn fill_numbers(&self, p: &Prepared, runs: &[TextRun], numbers: PageNumbers) -> Prepared {
        prepare::fill_numbers(p, runs, self.fonts, numbers)
    }

    fn min_content(&self, p: &Prepared, runs: &[TextRun], style: &ParaStyle) -> f32 {
        lines::min_content(p, runs, style, self.fonts)
    }
}

struct Entry {
    style: ParaStyle,
    prepared: Prepared,
    /// Widest unbreakable piece (min-content width).
    min_content: f32,
    layouts: Vec<(u32, Arc<LineLayout>)>,
}

impl Entry {
    fn new<S: TextShaper>(
        shaper: &S,
        runs: &[TextRun],
        style: ParaStyle,
        prepared: Prepared,
    ) -> Self {
        Self {
            min_content: shaper.min_content(&prepared, runs, &style),
            style,
            prepared,
            layouts: Vec::new(),
        }
    }

    fn cached(&self, width: f32) -> Option<Arc<LineLayout>> {
        let key = width_key(width);
        self.layouts
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, l)| l.clone())
    }

    fn break_lines<S: TextShaper>(&self, shaper: &S, runs: &[TextRun], width: f32) -> LineLayout {
        shaper.break_lines(&self.prepared, runs, &self.style, width.max(0.0))
    }

    /// The lines at `width`, cached.
    fn layout<S: TextShaper>(
        &mut self,
        shaper: &S,
        runs: &[TextRun],
        width: f32,
    ) -> Arc<LineLayout> {
        if let Some(l) = self.cached(width) {
            return l;
        }
        let layout = Arc::new(self.break_lines(shaper, runs, width));
        self.layouts.push((width_key(width), layout.clone()));
        layout
    }

    /// The lines at `width`: cached, or computed without caching.
    fn layout_at<S: TextShaper>(
        &self,
        shaper: &S,
        runs: &[TextRun],
        width: f32,
    ) -> Arc<LineLayout> {
        self.cached(width)
            .unwrap_or_else(|| Arc::new(self.break_lines(shaper, runs, width)))
    }

    /// The width to break lines at in Taffy's available space.
    fn width_in(&self, available: AvailableSpace) -> f32 {
        match available {
            AvailableSpace::Definite(w) => w,
            AvailableSpace::MaxContent => f32::INFINITY,
            // Truncating text can shrink to (almost) nothing.
            AvailableSpace::MinContent if self.style.ellipsis => 0.0,
            AvailableSpace::MinContent => self.min_content,
        }
    }
}

/// A text leaf's size from its lines at `width`. Taffy's `known` is the
/// border box; the width it makes available is already the content box
/// (the known width minus padding and border).
fn leaf_size(l: &LineLayout, known: Size<Option<f32>>, width: f32) -> Size<f32> {
    Size {
        width: known.width.unwrap_or(l.width.min(width.max(0.0))),
        height: known.height.unwrap_or(l.height),
    }
}

/// Shapes a text node into a cache entry.
fn new_entry<S: TextShaper>(
    shaper: &mut S,
    node: &Node,
    runs: &[TextRun],
    lang: Option<&str>,
) -> Entry {
    let style = ParaStyle::from_style(&node.style, lang);
    let prepared = shaper.prepare(runs, &style);
    Entry::new(shaper, runs, style, prepared)
}

/// Per-render cache of shaped paragraphs, keyed by node identity. Also
/// the layout engine's measure function for text leaves.
pub(crate) struct TextCache<'s, S: TextShaper> {
    pub shaper: S,
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
        let (fonts, hyphenator, lang) = (self.shaper.fonts, self.shaper.hyphenator, self.doc_lang);
        let faces = self.shaper.shapes.faces();
        let parts: Vec<(Vec<(usize, Entry)>, UnicodeMap)> = todo
            .par_chunks(chunk)
            .map(|nodes| {
                let mut shaper = RustybuzzShaper::with_faces(fonts, faces.clone());
                shaper.hyphenator = hyphenator;
                let entries = nodes
                    .iter()
                    .filter_map(|n| match &n.kind {
                        NodeKind::Text { runs } => {
                            Some((node_key(n), new_entry(&mut shaper, n, runs, lang)))
                        }
                        _ => None,
                    })
                    .collect();
                (entries, shaper.unicode)
            })
            .collect();
        for (entries, unicode) in parts {
            self.entries.extend(entries);
            for (font, map) in unicode.fonts {
                let target = self.shaper.unicode.fonts.entry(font).or_default();
                for (gid, text) in map {
                    target.entry(gid).or_insert(text);
                }
            }
        }
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
        let mut prepared = self.shaper.fill_numbers(&stand_in.prepared, runs, numbers);
        // The painter still draws the page's own digits: the same glyphs,
        // unless the paragraph ended up on another page than it was laid
        // out for.
        prepared.placeholders = true;
        let entry = Entry::new(&self.shaper, runs, stand_in.style.clone(), prepared);
        self.entries.insert(key, entry);
        true
    }

    fn entry(&mut self, node: &Node, runs: &[TextRun]) -> &mut Entry {
        let lang = self.doc_lang;
        let shaper = &mut self.shaper;
        self.entries
            .entry(node_key(node))
            .or_insert_with(|| new_entry(shaper, node, runs, lang))
    }

    pub fn layout(&mut self, node: &Node, width: f32) -> Option<Arc<LineLayout>> {
        let NodeKind::Text { runs } = &node.kind else {
            return None;
        };
        self.entry(node, runs);
        // Split borrows: the shaper is only read while breaking lines.
        let entry = self.entries.get_mut(&node_key(node))?;
        Some(entry.layout(&self.shaper, runs, width))
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
        Some((
            e.layout_at(&self.shaper, runs, width),
            &e.prepared,
            &e.style,
        ))
    }

    #[cfg(test)]
    pub fn prepared(&self, node: &Node) -> Option<(&Prepared, &ParaStyle)> {
        self.entries
            .get(&node_key(node))
            .map(|e| (&e.prepared, &e.style))
    }

    fn measure_text(
        &mut self,
        node: &Node,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let NodeKind::Text { runs } = &node.kind else {
            return Size::ZERO;
        };
        self.entry(node, runs);
        let Some(entry) = self.entries.get_mut(&node_key(node)) else {
            return Size::ZERO;
        };
        let width = entry.width_in(available.width);
        leaf_size(&entry.layout(&self.shaper, runs, width), known, width)
    }
}

impl<S: TextShaper> LeafMeasure for TextCache<'_, S> {
    fn measure(
        &mut self,
        node: &Node,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        match node.kind {
            NodeKind::Text { .. } => self.measure_text(node, known, available),
            _ => Size::ZERO,
        }
    }
}

/// The text of one page's header and footer, for laying them out again
/// with that page's numbers. Paragraphs with placeholders get the numbers
/// in place of their stand-in digits, and line layouts of their own; the
/// rest comes from the shared cache, which pages only read, so they can be
/// laid out in parallel.
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
        let layout = e.layout_at(&self.cache.shaper, runs, width);
        Some((layout, &e.prepared, &e.style))
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
        // Every header and footer paragraph was shaped before layout.
        let Some(shared) = self.cache.entries.get(&key) else {
            return Size::ZERO;
        };
        let shaper = &self.cache.shaper;
        if !shared.prepared.placeholders {
            let width = shared.width_in(available.width);
            return leaf_size(&shared.layout_at(shaper, runs, width), known, width);
        }
        let numbers = self.numbers;
        let entry = self.entries.entry(key).or_insert_with(|| {
            let prepared = shaper.fill_numbers(&shared.prepared, runs, numbers);
            Entry::new(shaper, runs, shared.style.clone(), prepared)
        });
        let width = entry.width_in(available.width);
        leaf_size(&entry.layout(shaper, runs, width), known, width)
    }
}

#[cfg(test)]
mod tests;
