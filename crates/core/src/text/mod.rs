//! Text: shaping (rustybuzz), line breaking (UAX #14), hyphenation
//! (hypher), and the per-render paragraph cache.
//!
//! Nothing outside this module sees shaping-library types: the rest of the
//! engine works with [`Prepared`] paragraphs and [`LineLayout`]s.

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

use taffy::prelude::{AvailableSpace, Size as TSize};

pub(crate) use paint::{PageNumbers, TextPainter};

use crate::fonts::{FontKey, FontSet};
use crate::layout::LeafMeasure;
use crate::schema::{Direction, Node, NodeKind, Style, TextAlign, TextRun, WhiteSpace};

pub(crate) const GLYPH_SPACE: u8 = 1;
pub(crate) const GLYPH_INVISIBLE: u8 = 2;

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
pub(crate) struct Ellipsis {
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
    pub hyphen: Option<Ellipsis>,
    pub ellipsis: Option<Ellipsis>,
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
    /// ISO 639-1 code when hyphenation is requested (patterns may still be
    /// missing; the hyphenator decides).
    pub hyphenate: Option<String>,
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
            hyphenate: if s.hyphenate == Some(true) {
                s.lang
                    .as_deref()
                    .or(doc_lang)
                    .and_then(hyphen::language_code)
            } else {
                None
            },
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
}

/// rustybuzz + unicode-linebreak + hypher implementation.
pub(crate) struct RustybuzzShaper<'f, 'a> {
    pub fonts: &'f FontSet<'a>,
    pub unicode: UnicodeMap,
    pub hyphenator: &'f dyn hyphen::Hyphenator,
    shapes: prepare::ShapeCache,
}

impl<'f, 'a> RustybuzzShaper<'f, 'a> {
    pub fn new(fonts: &'f FontSet<'a>) -> Self {
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
            shapes: prepare::ShapeCache::default(),
        }
    }
}

impl TextShaper for RustybuzzShaper<'_, '_> {
    fn prepare(&mut self, runs: &[TextRun], style: &ParaStyle) -> Prepared {
        let lang = style
            .hyphenate
            .as_deref()
            .filter(|l| self.hyphenator.supports(l));
        let opts = prepare::ParagraphOptions {
            white_space: style.white_space,
            direction: style.direction,
            hyphenate: lang,
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
}

/// Cache key of a layout width. Exact: bucketing could round below a
/// min-content width and change line breaks.
fn width_key(w: f32) -> u32 {
    if w.is_finite() {
        w.max(0.0).to_bits()
    } else {
        u32::MAX
    }
}

struct Entry {
    style: ParaStyle,
    prepared: Prepared,
    /// Widest unbreakable piece (min-content width).
    min_content: f32,
    layouts: Vec<(u32, Arc<LineLayout>)>,
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
    let mut min_content = 0.0f32;
    let mut start = 0usize;
    let bounds = prepared
        .breaks
        .iter()
        .map(|b| b.glyph as usize)
        .chain(std::iter::once(prepared.glyphs.len()));
    for end in bounds {
        let mut e = end;
        while e > start && prepared.glyphs[e - 1].flags & (GLYPH_SPACE | GLYPH_INVISIBLE) != 0 {
            e -= 1;
        }
        let w: f32 = prepared.glyphs[start..e].iter().map(|g| g.advance).sum();
        min_content = min_content.max(w);
        start = end;
    }
    min_content += style.indent.max(0.0);
    Entry {
        style,
        prepared,
        min_content,
        layouts: Vec::new(),
    }
}

/// Per-render cache of shaped paragraphs, keyed by node identity. Also
/// the layout engine's measure function for text leaves.
pub(crate) struct TextCache<'s, S: TextShaper> {
    pub shaper: S,
    doc_lang: Option<&'s str>,
    entries: HashMap<usize, Entry>,
}

fn node_key(n: &Node) -> usize {
    std::ptr::from_ref::<Node>(n) as usize
}

impl TextCache<'_, RustybuzzShaper<'_, '_>> {
    /// Shapes `nodes` (text nodes in document order) ahead of layout, in
    /// parallel. Each worker has its own shaper; glyph-to-Unicode maps are
    /// merged in document order, keeping the first mapping of each glyph,
    /// so the result does not depend on the thread count.
    pub fn prepare_all(&mut self, nodes: &[&Node]) {
        const MIN_CHUNK: usize = 64;
        let todo: Vec<&Node> = nodes
            .iter()
            .copied()
            .filter(|n| matches!(n.kind, NodeKind::Text { .. }))
            .filter(|n| !self.entries.contains_key(&node_key(n)))
            .collect();
        let threads = rayon::current_num_threads();
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
        let parts: Vec<(Vec<(usize, Entry)>, UnicodeMap)> = todo
            .par_chunks(chunk)
            .map(|nodes| {
                let mut shaper = RustybuzzShaper::new(fonts);
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
        }
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
        let key = width_key(width);
        // Split borrows: the shaper is only read while breaking lines.
        self.entry(node, runs);
        let entry = self.entries.get_mut(&node_key(node))?;
        if let Some((_, l)) = entry.layouts.iter().find(|(k, _)| *k == key) {
            return Some(l.clone());
        }
        let layout =
            Arc::new(
                self.shaper
                    .break_lines(&entry.prepared, runs, &entry.style, width.max(0.0)),
            );
        entry.layouts.push((key, layout.clone()));
        Some(layout)
    }

    /// Read-only [`layout`](Self::layout) for painting (which runs in
    /// parallel): the cached layout at `width`, or one computed without
    /// caching. Layout has already measured every painted text at its final
    /// width, so the fallback is rare.
    pub fn layout_at(&self, node: &Node, width: f32) -> Option<Arc<LineLayout>> {
        let NodeKind::Text { runs } = &node.kind else {
            return None;
        };
        let entry = self.entries.get(&node_key(node))?;
        let key = width_key(width);
        if let Some((_, l)) = entry.layouts.iter().find(|(k, _)| *k == key) {
            return Some(l.clone());
        }
        Some(Arc::new(self.shaper.break_lines(
            &entry.prepared,
            runs,
            &entry.style,
            width.max(0.0),
        )))
    }

    pub fn prepared(&self, node: &Node) -> Option<(&Prepared, &ParaStyle)> {
        self.entries
            .get(&node_key(node))
            .map(|e| (&e.prepared, &e.style))
    }

    fn measure_text(
        &mut self,
        node: &Node,
        known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32> {
        let NodeKind::Text { runs } = &node.kind else {
            return TSize::ZERO;
        };
        // Taffy's `known` is the border box; `available` is already the
        // content box (the known width minus padding and border).
        let width = match available.width {
            AvailableSpace::Definite(w) => w,
            AvailableSpace::MaxContent => f32::INFINITY,
            AvailableSpace::MinContent => {
                let e = self.entry(node, runs);
                if e.style.ellipsis {
                    // Truncating text can shrink to (almost) nothing.
                    0.0
                } else {
                    e.min_content
                }
            }
        };
        let Some(l) = self.layout(node, width) else {
            return TSize::ZERO;
        };
        TSize {
            width: known.width.unwrap_or(l.width.min(width.max(0.0))),
            height: known.height.unwrap_or(l.height),
        }
    }
}

impl<S: TextShaper> LeafMeasure for TextCache<'_, S> {
    fn measure(
        &mut self,
        node: &Node,
        known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32> {
        match node.kind {
            NodeKind::Text { .. } => self.measure_text(node, known, available),
            _ => TSize::ZERO,
        }
    }
}

#[cfg(test)]
mod tests;
