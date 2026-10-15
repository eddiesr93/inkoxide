//! Optimal line breaking (`textWrap: pretty`): Knuth and Plass's total-fit
//! algorithm over a chunk of a paragraph (the glyphs between two forced
//! breaks), choosing the breaks that make its lines most even, instead of
//! filling each line in turn.
//!
//! It works on the paragraph's break opportunities and the advances of its
//! glyphs, so anything that shapes to glyphs with advances (text and
//! inline objects) needs no special case. A chunk it cannot set
//! within its tolerances, or past its caps, is broken first fit instead.

use super::lines::Breaker;
use super::{BreakKind, BreakPoint, WIDTH_EPS};
use crate::schema::{TextAlign, TextWrap};

/// Most break opportunities in a chunk set optimally.
const MAX_BREAKS: usize = 20_000;
/// Most active breakpoints (lines that may start at a break) at a time.
const MAX_ACTIVE: usize = 512;
/// Most lines tried for a paragraph, over its chunks and passes.
const MAX_EVALUATIONS: u32 = 1 << 21;

/// Largest stretch ratio of the first pass, then of the second (TeX's
/// tolerances of 200 and about 100 000, as badness).
const TOLERANCES: [f64; 2] = [1.26, 10.0];
/// Demerits of every line (TeX's `\linepenalty`).
const LINE_PENALTY: f64 = 10.0;
/// Penalty of a break that shows a hyphen, or follows one.
const HYPHEN_PENALTY: f64 = 50.0;
/// Two lines in a row ending in a hyphen (`\doublehyphendemerits`).
const DOUBLE_HYPHEN: f64 = 10_000.0;
/// The line before the last ending in a hyphen (`\finalhyphendemerits`).
const FINAL_HYPHEN: f64 = 5_000.0;
/// Neighbouring lines more than one fitness class apart
/// (`\adjdemerits`).
const ADJACENT_FITNESS: f64 = 10_000.0;
/// Stretch of a line of ragged text, in ems of the paragraph's largest run.
const RAGGED_STRETCH: f32 = 2.0;
/// How much the spaces of a justified line may shrink, at most: a third
/// of their width (the painter allows as much).
pub(super) const SHRINK: f32 = 1.0 / 3.0;

/// Lines a paragraph may still try (see [`MAX_EVALUATIONS`]). Counted per
/// paragraph, not per render: paragraphs are broken in parallel, and the
/// output must not depend on which finishes first.
pub(super) struct Budget(u32);

impl Budget {
    pub fn new() -> Self {
        Self(MAX_EVALUATIONS)
    }
}

/// Why a chunk is broken first fit instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FirstFit {
    /// The paragraph is not `pretty`, or is not broken (`pre`, `nowrap`).
    NotPretty,
    /// The chunk fits on its line, or has no break opportunity.
    OneLine,
    /// It is justified, with no space to stretch (CJK).
    NoStretch,
    /// It has more than [`MAX_BREAKS`] break opportunities.
    TooManyBreaks,
    /// More than [`MAX_ACTIVE`] lines could start at once.
    TooManyActive,
    /// The paragraph tried [`MAX_EVALUATIONS`] lines.
    OutOfBudget,
    /// No lines fit within the tolerances: a word is wider than its line.
    NoFit,
}

/// A place a line may start: a break, with the best way found to reach it.
#[derive(Clone, Copy)]
struct Node {
    /// First glyph of the line that starts here.
    pos: usize,
    /// Index of that line in the paragraph.
    line: usize,
    fitness: u8,
    /// Top of that line, when line widths depend on it.
    y: f32,
    demerits: f64,
    /// The node the line ending here started at, and how it ended.
    prev: Option<usize>,
    kind: BreakKind,
    /// The line ending here shows or ends in a hyphen.
    flagged: bool,
}

/// Lines of a chunk: each with its first and end glyph and the break that
/// ends it.
pub(super) type Lines = Vec<(usize, usize, BreakKind)>;

/// The lines of glyphs `start..end`, set optimally, or why they are set
/// first fit: the paragraph is not `pretty`, first fit is the answer, or
/// it is the fallback.
pub(super) fn lines(
    b: &Breaker<'_, '_>,
    start: usize,
    end: usize,
    inside: &[BreakPoint],
    budget: &mut Budget,
) -> Result<Lines, FirstFit> {
    let para = b.para;
    let style = para.style;
    if style.text_wrap != TextWrap::Pretty || !style.wrap() {
        return Err(FirstFit::NotPretty);
    }
    if inside.len() > MAX_BREAKS {
        return Err(FirstFit::TooManyBreaks);
    }
    // A chunk that fits on its line is one line, at no cost.
    if inside.is_empty() || b.width(start, end) <= b.limit() + WIDTH_EPS {
        return Err(FirstFit::OneLine);
    }
    let chunk = Chunk::new(b, start, end);
    // Justified text without spaces (CJK) has nothing to stretch.
    if chunk.justify && chunk.space_prefix.last() == chunk.space_prefix.first() {
        return Err(FirstFit::NoStretch);
    }
    let mut why = FirstFit::NoFit;
    for tolerance in TOLERANCES {
        match chunk.pass(b, inside, tolerance, budget) {
            Ok(lines) => return Ok(lines),
            Err(e) => why = e,
        }
    }
    Err(why)
}

/// What a pass over a chunk measures lines with.
struct Chunk {
    start: usize,
    end: usize,
    justify: bool,
    /// `space_prefix[i]`: advance of the spaces among glyphs
    /// `start..start + i`.
    space_prefix: Vec<f32>,
    /// Stretch of a ragged line.
    ragged: f32,
}

impl Chunk {
    fn new(b: &Breaker<'_, '_>, start: usize, end: usize) -> Self {
        let para = b.para;
        let mut space_prefix = Vec::with_capacity(end - start + 1);
        space_prefix.push(0.0f32);
        for g in &para.prepared.glyphs[start..end] {
            let space = if g.is_space() { g.advance } else { 0.0 };
            space_prefix.push(space_prefix[space_prefix.len() - 1] + space);
        }
        let largest = para.runs.iter().map(|r| r.size).fold(0.0f32, f32::max);
        Self {
            start,
            end,
            justify: para.style.align == TextAlign::Justify,
            space_prefix,
            ragged: RAGGED_STRETCH * largest,
        }
    }

    /// Advance of the spaces among glyphs `s..e`.
    fn space_advance(&self, s: usize, e: usize) -> f32 {
        self.space_prefix[e - self.start] - self.space_prefix[s - self.start]
    }

    /// Whether glyph `at - 1` is a hyphen, so that a break before `at`
    /// follows one.
    fn after_hyphen(b: &Breaker<'_, '_>, at: usize) -> bool {
        let p = b.para.prepared;
        let Some(seg) = at.checked_sub(1).and_then(|i| p.segment_of(i)) else {
            return false;
        };
        let font = b.para.fonts.get(seg.font);
        let id = p.glyphs[at - 1].id;
        font.glyph('-') == Some(id) || font.glyph('\u{2010}') == Some(id)
    }

    /// One pass at `tolerance`: the best lines, or why there are none:
    /// some line cannot be set within it, or a cap is reached.
    fn pass(
        &self,
        b: &Breaker<'_, '_>,
        inside: &[BreakPoint],
        tolerance: f64,
        budget: &mut Budget,
    ) -> Result<Lines, FirstFit> {
        let p = b.para.prepared;
        let banded = b.banded();
        // Lines below every band, after the first, are all as wide: their
        // breakpoints merge whatever their line numbers (TeX's easy lines).
        // Above the last band's bottom, a line's width depends on where it
        // starts, so its number stays.
        let bands_end = b.bands_end();
        let line_key = |line: usize, y: f32| {
            if banded && y < bands_end {
                line
            } else {
                line.min(1)
            }
        };
        let mut nodes = vec![Node {
            pos: self.start,
            line: b.count(),
            fitness: 1,
            y: b.top(),
            demerits: 0.0,
            prev: None,
            kind: BreakKind::Allowed,
            flagged: false,
        }];
        let mut active: Vec<usize> = vec![0];
        let candidates = inside
            .iter()
            .filter(|br| br.glyph as usize > self.start && (br.glyph as usize) < self.end)
            .map(|br| (br.glyph as usize, br.kind))
            .chain(std::iter::once((self.end, BreakKind::Mandatory)));
        for (pos, kind) in candidates {
            let forced = kind == BreakKind::Mandatory;
            let flagged = kind == BreakKind::Hyphen || (!forced && Self::after_hyphen(b, pos));
            let penalty = if flagged { HYPHEN_PENALTY } else { 0.0 };
            // Best way to break here, per (line, fitness class).
            let mut best: Vec<(usize, u8, f64, usize, f32)> = Vec::new();
            let mut done: Vec<usize> = Vec::new();
            for &a in &active {
                if budget.0 == 0 {
                    return Err(FirstFit::OutOfBudget);
                }
                budget.0 -= 1;
                let from = nodes[a];
                let limit = b.limit_at(from.line, from.y);
                let width = b.width(from.pos, pos) + b.hyphen_width(pos, kind);
                let ve = p.visible_end(from.pos, pos);
                let space_advance = self.space_advance(from.pos, ve);
                let ratio = self.ratio(limit, width, space_advance, forced);
                if ratio < -1.0 {
                    // Too wide, and every later break is wider still.
                    done.push(a);
                    continue;
                }
                if ratio > tolerance {
                    continue;
                }
                let fitness: u8 = match ratio {
                    r if r < -0.5 => 0,
                    r if r <= 0.5 => 1,
                    r if r <= 1.0 => 2,
                    _ => 3,
                };
                let badness = 100.0 * ratio.abs().powi(3);
                let mut demerits = (LINE_PENALTY + badness).powi(2) + penalty * penalty;
                if flagged && from.flagged {
                    demerits += DOUBLE_HYPHEN;
                }
                if forced && from.flagged {
                    demerits += FINAL_HYPHEN;
                }
                if fitness.abs_diff(from.fitness) > 1 {
                    demerits += ADJACENT_FITNESS;
                }
                let total = from.demerits + demerits;
                // Where the next line starts.
                let y = if banded {
                    from.y + b.height(from.pos, pos)
                } else {
                    from.y
                };
                let key = line_key(from.line + 1, y);
                match best.iter_mut().find(|c| c.0 == key && c.1 == fitness) {
                    // Ties go to the earliest start.
                    Some(c) if total < c.2 => *c = (key, fitness, total, a, y),
                    Some(_) => {}
                    None => best.push((key, fitness, total, a, y)),
                }
            }
            active.retain(|a| !done.contains(a));
            for (_, fitness, total, a, y) in best {
                let from = nodes[a];
                nodes.push(Node {
                    pos,
                    line: from.line + 1,
                    fitness,
                    y,
                    demerits: total,
                    prev: Some(a),
                    kind,
                    flagged,
                });
                active.push(nodes.len() - 1);
            }
            if active.is_empty() {
                return Err(FirstFit::NoFit);
            }
            if active.len() > MAX_ACTIVE {
                return Err(FirstFit::TooManyActive);
            }
        }
        // The best way to the end of the chunk; ties go to the first found.
        let last = (0..nodes.len())
            .filter(|&k| nodes[k].pos == self.end)
            .min_by(|&x, &y| nodes[x].demerits.total_cmp(&nodes[y].demerits))
            .ok_or(FirstFit::NoFit)?;
        let mut out = Vec::new();
        let mut k = last;
        while let Some(prev) = nodes[k].prev {
            out.push((nodes[prev].pos, nodes[k].pos, nodes[k].kind));
            k = prev;
        }
        out.reverse();
        Ok(out)
    }

    /// How far a line of natural `width` stretches (positive) or shrinks
    /// (negative) to fill `limit`, in units of what it can: below -1, it
    /// does not fit.
    fn ratio(&self, limit: f32, width: f32, space_advance: f32, forced: bool) -> f64 {
        let slack = f64::from(limit - width);
        if slack.abs() <= f64::from(WIDTH_EPS) {
            return 0.0;
        }
        if slack > 0.0 {
            // The last line of a chunk is not stretched: it may end anywhere.
            if forced {
                return 0.0;
            }
            let stretch = if self.justify {
                space_advance / 2.0
            } else {
                self.ragged
            };
            return if stretch > 0.0 {
                slack / f64::from(stretch)
            } else {
                f64::INFINITY
            };
        }
        // A justified line may also shrink its spaces, but the last line of
        // a chunk is not justified.
        let shrink = if self.justify && !forced {
            space_advance * SHRINK
        } else {
            0.0
        };
        if shrink > 0.0 {
            slack / f64::from(shrink)
        } else {
            f64::NEG_INFINITY
        }
    }
}
