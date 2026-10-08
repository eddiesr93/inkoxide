//! Text: shaping (rustybuzz), line breaking (UAX #14), hyphenation
//! (hypher), and the per-render paragraph cache.
//!
//! Nothing outside this module sees shaping-library types: the rest of the
//! engine works with [`Prepared`] paragraphs and [`LineLayout`]s.

mod hyphen;
mod lines;
mod paint;
mod prepare;

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::rc::Rc;

use taffy::prelude::{AvailableSpace, Size as TSize};

pub(crate) use paint::{PageNumbers, TextPainter};

use crate::fonts::{FontKey, FontSet};
use crate::layout::LeafMeasure;
use crate::schema::{Node, NodeKind, Style, TextAlign, TextRun, WhiteSpace};

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
    pub text: Range<u32>,
    pub glyphs: Range<u32>,
}

impl Segment {
    fn new(run: usize, font: FontKey, _r: &TextRun, text: Range<usize>) -> Self {
        Self {
            run: run as u16,
            font,
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
    pub text: String,
    pub segments: Vec<Segment>,
    pub glyphs: Vec<Glyph>,
    pub breaks: Vec<BreakPoint>,
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
    /// ISO 639-1 code when hyphenation is requested (patterns may still be
    /// missing; the hyphenator decides).
    pub hyphenate: Option<String>,
}

impl ParaStyle {
    /// Whether lines wrap at soft break opportunities.
    pub fn wrap(&self) -> bool {
        self.white_space == WhiteSpace::Normal
    }

    pub fn from_style(s: &Style, doc_lang: Option<&str>) -> Self {
        let white_space = s.white_space.unwrap_or(WhiteSpace::Normal);
        Self {
            align: s.text_align.unwrap_or(TextAlign::Left),
            line_height: s.line_height,
            max_lines: s.max_lines,
            ellipsis: s.ellipsis.unwrap_or(false),
            white_space,
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

/// Glyph → text mappings per font, for the PDF ToUnicode CMaps.
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

/// Dependency firewall around text shaping (§5.1).
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
    pub hyphenator: Box<dyn hyphen::Hyphenator>,
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
            hyphenator: Box::new(hyphen::HypherHyphenator),
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
            hyphenate: lang,
            hyphenator: self.hyphenator.as_ref(),
            runs,
        };
        prepare::prepare(&opts, self.fonts, &mut self.unicode)
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
    layouts: Vec<(u32, Rc<LineLayout>)>,
}

/// Per-render cache of shaped paragraphs, keyed by node identity. Also
/// the layout engine's measure function for text leaves.
pub(crate) struct TextCache<'s, S: TextShaper> {
    pub shaper: S,
    doc_lang: Option<&'s str>,
    entries: HashMap<usize, Entry>,
}

fn node_key(n: &Node) -> usize {
    n as *const Node as usize
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
        self.entries.entry(node_key(node)).or_insert_with(|| {
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
                while e > start
                    && prepared.glyphs[e - 1].flags & (GLYPH_SPACE | GLYPH_INVISIBLE) != 0
                {
                    e -= 1;
                }
                let w: f32 = prepared.glyphs[start..e].iter().map(|g| g.advance).sum();
                min_content = min_content.max(w);
                start = end;
            }
            Entry {
                style,
                prepared,
                min_content,
                layouts: Vec::new(),
            }
        })
    }

    /// Line layout of text `node` at `width` (cached per 0.5 pt bucket).
    pub fn layout(&mut self, node: &Node, width: f32) -> Option<Rc<LineLayout>> {
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
            Rc::new(
                self.shaper
                    .break_lines(&entry.prepared, runs, &entry.style, width.max(0.0)),
            );
        entry.layouts.push((key, layout.clone()));
        Some(layout)
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
        let width = match (known.width, available.width) {
            (Some(w), _) => w,
            (None, AvailableSpace::Definite(w)) => w,
            (None, AvailableSpace::MaxContent) => f32::INFINITY,
            (None, AvailableSpace::MinContent) => {
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
