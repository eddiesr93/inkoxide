//! Line breaking (greedy, first fit) and line metrics.

use super::{
    BreakKind, Ellipsis, Line, LineLayout, ParaStyle, Prepared, GLYPH_INVISIBLE, GLYPH_SPACE,
};
use crate::fonts::FontSet;
use crate::schema::TextRun;

const ELLIPSIS: char = '\u{2026}';
const HYPHEN: char = '\u{2010}';

/// Tolerance for float noise when comparing widths.
const EPS: f32 = 1e-3;

/// Breaks `p` into lines no wider than `max_width` (`f32::INFINITY` for
/// max-content).
pub(crate) fn break_lines(
    p: &Prepared,
    runs: &[TextRun],
    style: &ParaStyle,
    fonts: &FontSet<'_>,
    max_width: f32,
) -> LineLayout {
    if p.glyphs.is_empty() {
        return LineLayout::default();
    }
    let n = p.glyphs.len();
    // prefix[i] = advance of glyphs[0..i].
    let mut prefix = Vec::with_capacity(n + 1);
    prefix.push(0.0f32);
    for g in &p.glyphs {
        prefix.push(prefix[prefix.len() - 1] + g.advance);
    }

    // Width of glyphs[start..end] without trailing spaces / invisibles.
    let visible_end = |start: usize, end: usize| {
        let mut e = end;
        while e > start && p.glyphs[e - 1].flags & (GLYPH_SPACE | GLYPH_INVISIBLE) != 0 {
            e -= 1;
        }
        e
    };
    let width = |start: usize, end: usize| prefix[visible_end(start, end)] - prefix[start];
    let hyphen_width = |end: usize| hyphen_for(p, fonts, runs, end).map_or(0.0, |h| h.advance);

    let mut raw: Vec<(usize, usize, BreakKind)> = Vec::new();
    let mut start = 0usize;
    let mut last_fit: Option<(usize, BreakKind)> = None;
    let mut bi = 0usize;
    let breaks = &p.breaks;
    loop {
        let (pos, kind) = match breaks.get(bi) {
            Some(b) => (b.glyph as usize, b.kind),
            None => (n, BreakKind::Mandatory),
        };
        if pos <= start {
            bi += 1;
            continue;
        }
        let extra = if kind == BreakKind::Hyphen {
            hyphen_width(pos)
        } else {
            0.0
        };
        let fits = width(start, pos) + extra <= max_width + EPS;
        if fits {
            if kind == BreakKind::Mandatory {
                raw.push((start, pos, kind));
                start = pos;
                last_fit = None;
                if pos >= n {
                    break;
                }
            } else {
                last_fit = Some((pos, kind));
            }
            bi += 1;
            continue;
        }
        match last_fit.take() {
            Some((lp, lk)) => {
                raw.push((start, lp, lk));
                start = lp;
                // Re-examine the same opportunity against the new start.
            }
            None if !style.wrap() => {
                // pre / nowrap: overflow instead of breaking words.
                raw.push((start, pos, kind));
                start = pos;
                bi += 1;
                if pos >= n {
                    break;
                }
            }
            None => {
                // A single unbreakable word wider than the line: break it
                // between clusters (overflow-wrap: anywhere).
                let mut end = start + 1;
                while end < pos
                    && (p.glyphs[end].cluster == p.glyphs[end - 1].cluster
                        || width(start, end + 1) <= max_width + EPS)
                {
                    end += 1;
                }
                // Never split a cluster.
                while end < pos && p.glyphs[end].cluster == p.glyphs[end - 1].cluster {
                    end += 1;
                }
                if end >= pos {
                    raw.push((start, pos, kind));
                    start = pos;
                    bi += 1;
                    if pos >= n {
                        break;
                    }
                } else {
                    raw.push((start, end, BreakKind::Allowed));
                    start = end;
                }
            }
        }
    }

    let mut lines: Vec<Line> = raw
        .into_iter()
        .map(|(s, e, kind)| {
            let hyphen = if kind == BreakKind::Hyphen && e < n {
                hyphen_for(p, fonts, runs, e)
            } else {
                None
            };
            let ve = visible_end(s, e);
            let mut line = Line {
                start: s as u32,
                end: ve as u32,
                width: prefix[ve] - prefix[s] + hyphen.as_ref().map_or(0.0, |h| h.advance),
                hyphen,
                ellipsis: None,
                forced: kind == BreakKind::Mandatory,
                top: 0.0,
                ascent: 0.0,
                height: 0.0,
            };
            metrics(&mut line, p, runs, style, fonts, s);
            line
        })
        .collect();

    if let Some(max) = style.max_lines {
        let max = max as usize;
        let truncated = lines.len() > max;
        lines.truncate(max);
        if style.ellipsis {
            if let Some(last) = lines.last_mut() {
                if truncated || last.width > max_width + EPS {
                    apply_ellipsis(last, p, runs, fonts, max_width, &prefix);
                }
            }
        }
    }

    let mut top = 0.0;
    let mut max_w = 0.0f32;
    for l in &mut lines {
        l.top = top;
        top += l.height;
        max_w = max_w.max(l.width);
    }
    LineLayout {
        lines,
        width: max_w,
        height: top,
    }
}

/// The hyphen shown when breaking before glyph `at`, in the font of the
/// glyph before it.
fn hyphen_for(p: &Prepared, fonts: &FontSet<'_>, runs: &[TextRun], at: usize) -> Option<Ellipsis> {
    let seg = p.segment_of(at.checked_sub(1)?)?;
    let font = fonts.get(seg.font);
    let size = runs[seg.run as usize].size;
    let gid = font.glyph(HYPHEN).or_else(|| font.glyph('-'))?;
    Some(Ellipsis {
        font: seg.font,
        run: seg.run,
        glyph: gid,
        count: 1,
        advance: font.advance(gid) * font.scale(size),
    })
}

fn apply_ellipsis(
    line: &mut Line,
    p: &Prepared,
    runs: &[TextRun],
    fonts: &FontSet<'_>,
    max_width: f32,
    prefix: &[f32],
) {
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
        && (prefix[e] - prefix[s] + ell_w > max_width + EPS
            || p.glyphs[e - 1].flags & GLYPH_SPACE != 0)
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
    line.ellipsis = Some(Ellipsis {
        font: seg.font,
        run: seg.run,
        glyph: gid,
        count,
        advance: ell_w / f32::from(count),
    });
}

/// Line box height and baseline from the fonts and sizes on the line
/// (CSS inline formatting with half-leading).
fn metrics(
    line: &mut Line,
    p: &Prepared,
    runs: &[TextRun],
    style: &ParaStyle,
    fonts: &FontSet<'_>,
    start: usize,
) {
    let (mut above, mut below) = (0.0f32, 0.0f32);
    let mut add = |seg: &super::Segment| {
        let run = &runs[seg.run as usize];
        let m = &fonts.get(seg.font).data.metrics;
        let scale = run.size / m.units_per_em;
        let asc = m.ascender * scale;
        let desc = -m.descender * scale;
        let (a, b) = if let Some(lh) = style.line_height {
            let half_leading = (lh * run.size - (asc + desc)) / 2.0;
            (asc + half_leading, desc + half_leading)
        } else {
            let gap = m.line_gap * scale / 2.0;
            (asc + gap, desc + gap)
        };
        above = above.max(a);
        below = below.max(b);
    };
    let (s, e) = (line.start as usize, (line.end as usize).max(start));
    let mut any = false;
    for seg in &p.segments {
        let (gs, ge) = (seg.glyphs.start as usize, seg.glyphs.end as usize);
        if gs < e && ge > s {
            add(seg);
            any = true;
        }
    }
    if !any {
        // Empty line (e.g. between two newlines): metrics of the segment the
        // line starts in, else the first one.
        if let Some(seg) = p.segment_of(start).or(p.segments.first()) {
            add(seg);
        }
    }
    line.ascent = above;
    line.height = above + below;
}
