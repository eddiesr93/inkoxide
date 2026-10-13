//! Line breaking (greedy, first fit) and line metrics.

use super::{baseline_shift, BreakKind, BreakPoint, ExtraGlyph, Line, LineLayout, Para, WIDTH_EPS};
use crate::layout::LineBoxes;

/// Shown at the end of a truncated line; three dots in fonts without it.
pub(super) const ELLIPSIS: char = '\u{2026}';
/// Shown at a hyphenation break; a hyphen-minus in fonts without it.
pub(super) const HYPHEN: char = '\u{2010}';

/// Breaks `para` into lines in `boxes`: no wider than their box
/// (`f32::INFINITY` wide for max-content), each chunk between forced breaks
/// on its own.
pub(crate) fn break_lines(para: &Para<'_>, boxes: &LineBoxes) -> LineLayout {
    let p = para.prepared;
    if p.glyphs.is_empty() {
        return LineLayout::default();
    }
    let mut b = Breaker::new(para, boxes);
    let n = p.glyphs.len();
    let mut start = 0usize;
    let mut first = 0usize;
    // Chunks end at each forced break and at the end of the paragraph.
    let ends = p
        .breaks
        .iter()
        .enumerate()
        .filter(|(_, br)| br.kind == BreakKind::Mandatory)
        .map(|(i, br)| (br.glyph as usize, i))
        .chain(std::iter::once((n, p.breaks.len())));
    for (end, last) in ends {
        if end <= start {
            continue;
        }
        // The opportunities inside the chunk.
        let inside = &p.breaks[first..last];
        first = (last + 1).min(p.breaks.len());
        b.greedy(start, end, inside);
        start = end;
    }
    b.finish()
}

/// Lines being broken: where the next one goes, and what measures them.
pub(super) struct Breaker<'a, 'p> {
    pub para: &'a Para<'p>,
    boxes: &'a LineBoxes,
    /// `prefix[i]` = advance of glyphs `0..i`.
    pub prefix: Vec<f32>,
    /// Height of the tallest line the paragraph can have: a line looks for
    /// its box over that height, as it is not known before its glyphs.
    tallest: f32,
    lines: Vec<Line>,
    /// Each line's box (left edge, width), when the boxes have bands.
    line_boxes: Vec<(f32, f32)>,
    /// Top of the next line.
    y: f32,
}

impl<'a, 'p> Breaker<'a, 'p> {
    fn new(para: &'a Para<'p>, boxes: &'a LineBoxes) -> Self {
        let p = para.prepared;
        let mut prefix = Vec::with_capacity(p.glyphs.len() + 1);
        prefix.push(0.0f32);
        for g in &p.glyphs {
            prefix.push(prefix[prefix.len() - 1] + g.advance);
        }
        let tallest = if boxes.banded() {
            let mut all = empty_line(0, p.glyphs.len());
            metrics(&mut all, para);
            all.height
        } else {
            0.0
        };
        Self {
            para,
            boxes,
            prefix,
            tallest,
            lines: Vec::new(),
            line_boxes: Vec::new(),
            y: 0.0,
        }
    }

    /// Width of glyphs `start..end` without their trailing spaces and
    /// invisible glyphs.
    pub fn width(&self, start: usize, end: usize) -> f32 {
        self.prefix[self.para.prepared.visible_end(start, end)] - self.prefix[start]
    }

    /// Width of the hyphen a line ending at a break of `kind` before glyph
    /// `at` shows.
    pub fn hyphen_width(&self, at: usize, kind: BreakKind) -> f32 {
        if kind == BreakKind::Hyphen {
            hyphen_for(self.para, at).map_or(0.0, |h| h.advance)
        } else {
            0.0
        }
    }

    /// The box of the next line, at the current top: its left edge and
    /// its width.
    fn next_box(&self) -> (f32, f32) {
        if self.boxes.banded() {
            self.boxes.at(self.y, self.tallest)
        } else {
            (0.0, self.boxes.width)
        }
    }

    /// The room for the glyphs of the next line: its box, less the indent
    /// on the first line (which a hanging indent widens).
    pub fn limit(&self) -> f32 {
        let w = self.next_box().1;
        if self.lines.is_empty() {
            w - self.para.style.indent
        } else {
            w
        }
    }

    /// Moves the next line down to where the boxes change, when a band is
    /// left below it. Each move goes to a band edge below the last, and
    /// there are finitely many, so moving down ends.
    fn move_down(&mut self) -> bool {
        match self.boxes.next_change(self.y) {
            Some(y) => {
                self.y = y;
                true
            }
            None => false,
        }
    }

    /// Adds the line of glyphs `s..e`, ended by a break of `kind`.
    pub fn push(&mut self, s: usize, e: usize, kind: BreakKind) {
        let Para {
            prepared: p, style, ..
        } = *self.para;
        let n = p.glyphs.len();
        let hyphen = if kind == BreakKind::Hyphen && e < n {
            hyphen_for(self.para, e)
        } else {
            None
        };
        let ve = p.visible_end(s, e);
        let (left, avail) = self.next_box();
        let mut line = Line {
            width: self.prefix[ve] - self.prefix[s] + hyphen.as_ref().map_or(0.0, |h| h.advance),
            hyphen,
            forced: kind == BreakKind::Mandatory,
            indent: if s == 0 { style.indent } else { 0.0 },
            top: self.y,
            ..empty_line(s, ve)
        };
        metrics(&mut line, self.para);
        self.y += line.height;
        if self.boxes.banded() {
            self.line_boxes.push((left, avail));
        }
        self.lines.push(line);
    }

    /// Breaks glyphs `start..end`, a chunk ended by a forced break or the
    /// end of the paragraph, first fit: each line takes as much as fits,
    /// at the opportunities `inside` the chunk.
    pub fn greedy(&mut self, start: usize, end: usize, inside: &[BreakPoint]) {
        let p = self.para.prepared;
        let mut start = start;
        let mut last_fit: Option<(usize, BreakKind)> = None;
        let mut bi = 0usize;
        loop {
            let (pos, kind) = match inside.get(bi) {
                Some(b) => (b.glyph as usize, b.kind),
                None => (end, BreakKind::Mandatory),
            };
            if pos <= start {
                bi += 1;
                continue;
            }
            let limit = self.limit();
            let fits = self.width(start, pos) + self.hyphen_width(pos, kind) <= limit + WIDTH_EPS;
            if fits {
                if kind == BreakKind::Mandatory {
                    self.push(start, pos, kind);
                    return;
                }
                last_fit = Some((pos, kind));
                bi += 1;
                continue;
            }
            match last_fit.take() {
                Some((lp, lk)) => {
                    self.push(start, lp, lk);
                    start = lp;
                    // Re-examine the same opportunity against the new start.
                }
                // Beside a float, a line that does not fit moves down to
                // where there is more room (CSS 2.2 §9.5).
                None if self.move_down() => {}
                None if !self.para.style.wrap() => {
                    // pre / nowrap: overflow instead of breaking words.
                    self.push(start, pos, kind);
                    if kind == BreakKind::Mandatory {
                        return;
                    }
                    start = pos;
                    bi += 1;
                }
                None => {
                    // A single unbreakable word wider than the line: break it
                    // between clusters (overflow-wrap: anywhere).
                    let mut e = start + 1;
                    while e < pos
                        && (p.glyphs[e].cluster == p.glyphs[e - 1].cluster
                            || self.width(start, e + 1) <= limit + WIDTH_EPS)
                    {
                        e += 1;
                    }
                    // Never split a cluster.
                    while e < pos && p.glyphs[e].cluster == p.glyphs[e - 1].cluster {
                        e += 1;
                    }
                    if e >= pos {
                        self.push(start, pos, kind);
                        if kind == BreakKind::Mandatory {
                            return;
                        }
                        start = pos;
                        bi += 1;
                    } else {
                        self.push(start, e, BreakKind::Allowed);
                        start = e;
                    }
                }
            }
        }
    }

    /// The lines, cut to `maxLines` with an ellipsis, and their extent.
    fn finish(self) -> LineLayout {
        let Breaker {
            para,
            boxes,
            prefix,
            mut lines,
            mut line_boxes,
            ..
        } = self;
        let style = para.style;
        if let Some(max) = style.max_lines {
            let max = max as usize;
            let truncated = lines.len() > max;
            lines.truncate(max);
            line_boxes.truncate(max);
            if style.ellipsis {
                let width = line_boxes.last().map_or(boxes.width, |b| b.1);
                if let Some(last) = lines.last_mut() {
                    let limit = width - last.indent;
                    if truncated || last.width > limit + WIDTH_EPS {
                        apply_ellipsis(last, para, limit, &prefix);
                    }
                }
            }
        }
        let mut max_w = 0.0f32;
        for (i, l) in lines.iter().enumerate() {
            let w = l.width + l.indent;
            max_w = max_w.max(line_boxes.get(i).map_or(w, |b| b.0 + w));
        }
        LineLayout {
            height: lines.last().map_or(0.0, |l| l.top + l.height),
            lines,
            width: max_w,
            boxes: line_boxes,
        }
    }
}

/// A line of glyphs `start..end` with nothing measured yet.
fn empty_line(start: usize, end: usize) -> Line {
    Line {
        start: start as u32,
        end: end as u32,
        width: 0.0,
        hyphen: None,
        ellipsis: None,
        forced: false,
        indent: 0.0,
        top: 0.0,
        ascent: 0.0,
        height: 0.0,
    }
}

/// Widest unbreakable piece of a paragraph, first-line indent included. A
/// piece that ends at a hyphenation point includes the hyphen shown there.
pub(crate) fn min_content(para: &Para<'_>) -> f32 {
    let Para {
        prepared: p, style, ..
    } = *para;
    let mut min_content = 0.0f32;
    let mut start = 0usize;
    let ends = p
        .breaks
        .iter()
        .map(|b| (b.glyph as usize, b.kind))
        .chain(std::iter::once((p.glyphs.len(), BreakKind::Mandatory)));
    for (end, kind) in ends {
        let e = p.visible_end(start, end);
        let mut w: f32 = p.glyphs[start..e].iter().map(|g| g.advance).sum();
        if kind == BreakKind::Hyphen && e > start {
            w += hyphen_for(para, end).map_or(0.0, |h| h.advance);
        }
        min_content = min_content.max(w);
        start = end;
    }
    min_content + style.indent.max(0.0)
}

/// The hyphen shown when breaking before glyph `at`, in the font of the
/// glyph before it.
fn hyphen_for(para: &Para<'_>, at: usize) -> Option<ExtraGlyph> {
    let seg = para.prepared.segment_of(at.checked_sub(1)?)?;
    let font = para.fonts.get(seg.font);
    let size = para.runs[seg.run as usize].size;
    let gid = font.glyph(HYPHEN).or_else(|| font.glyph('-'))?;
    Some(ExtraGlyph {
        font: seg.font,
        run: seg.run,
        glyph: gid,
        count: 1,
        advance: font.advance(gid) * font.scale(size),
    })
}

fn apply_ellipsis(line: &mut Line, para: &Para<'_>, max_width: f32, prefix: &[f32]) {
    let Para {
        prepared: p,
        runs,
        fonts,
        ..
    } = *para;
    let s = line.start as usize;
    let last = (line.end as usize).max(s + 1) - 1;
    let Some(seg) = p.segment_of(last) else {
        return;
    };
    let font = fonts.get(seg.font);
    let size = runs[seg.run as usize].size;
    let (gid, count) = match font.glyph(ELLIPSIS) {
        Some(g) => (g, 1),
        None => match font.glyph('.') {
            Some(g) => (g, 3),
            None => return,
        },
    };
    let ell_w = font.advance(gid) * font.scale(size) * f32::from(count);
    let mut e = line.end as usize;
    while e > s
        && (prefix[e] - prefix[s] + ell_w > max_width + WIDTH_EPS || p.glyphs[e - 1].is_space())
    {
        e -= 1;
        // Keep whole clusters.
        while e > s && p.glyphs[e].cluster == p.glyphs[e - 1].cluster {
            e -= 1;
        }
    }
    line.end = e as u32;
    line.hyphen = None;
    line.width = prefix[e] - prefix[s] + ell_w;
    line.ellipsis = Some(ExtraGlyph {
        font: seg.font,
        run: seg.run,
        glyph: gid,
        count,
        advance: ell_w / f32::from(count),
    });
}

/// Line box height and baseline from the fonts, sizes and baseline shifts
/// on the line (CSS inline formatting with half-leading).
fn metrics(line: &mut Line, para: &Para<'_>) {
    let Para {
        prepared: p,
        runs,
        style,
        fonts,
    } = *para;
    let (mut above, mut below) = (0.0f32, 0.0f32);
    let mut add = |seg: &super::Segment| {
        let run = &runs[seg.run as usize];
        let font = fonts.get(seg.font);
        let m = &font.data.metrics;
        let scale = font.scale(run.size);
        let asc = m.ascender * scale;
        let desc = -m.descender * scale;
        let (a, b) = if let Some(lh) = style.line_height {
            let half_leading = (lh * run.size - (asc + desc)) / 2.0;
            (asc + half_leading, desc + half_leading)
        } else {
            let gap = m.line_gap * scale / 2.0;
            (asc + gap, desc + gap)
        };
        // A raised run reaches higher, a lowered one lower.
        let shift = baseline_shift(run, fonts);
        above = above.max(a + shift);
        below = below.max(b - shift);
    };
    let s = line.start as usize;
    let e = (line.end as usize).max(s);
    let mut any = false;
    // Segments are in glyph order: those on the line follow the first
    // that ends after its start.
    let from = p
        .segments
        .partition_point(|seg| seg.glyphs.end as usize <= s);
    for seg in p.segments[from..]
        .iter()
        .take_while(|seg| (seg.glyphs.start as usize) < e)
    {
        if seg.glyphs.end as usize > s {
            add(seg);
            any = true;
        }
    }
    if !any {
        // Empty line (e.g. between two newlines): metrics of the segment the
        // line starts in, else the first one.
        if let Some(seg) = p.segment_of(s).or(p.segments.first()) {
            add(seg);
        }
    }
    line.ascent = above;
    line.height = above + below;
}
