//! Text nodes → `Op::Text` (plus decorations), with alignment,
//! justification and page-number placeholders.

use super::{Ellipsis, Line, Prepared, TextCache, TextShaper, GLYPH_INVISIBLE, GLYPH_SPACE};
use crate::fonts::FontSet;
use crate::layout::Geometry;
use crate::paint::{ContentCtx, ContentPainter, Op, Rect, TextGlyph, TextOp};
use crate::schema::{Color, Decoration, Node, NodeKind, Placeholder, TextAlign, TextRun};

const BLACK: Color = [0.0, 0.0, 0.0, 1.0];

/// Page numbers substituted for placeholders on the page being painted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageNumbers {
    pub current: u32,
    pub total: u32,
}

pub(crate) struct TextPainter<'c, 's, 'f, 'a, S: TextShaper> {
    pub cache: &'c TextCache<'s, S>,
    pub fonts: &'f FontSet<'a>,
    pub numbers: PageNumbers,
}

impl<S: TextShaper> ContentPainter for TextPainter<'_, '_, '_, '_, S> {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
        let NodeKind::Text { runs } = &node.kind else {
            return;
        };
        let (cx, cy, cw, _) = geo.content_box();
        let Some(layout) = self.cache.layout_at(node, cw) else {
            return;
        };
        let Some((prepared, style)) = self.cache.prepared(node) else {
            return;
        };
        for (li, line) in layout.lines.iter().enumerate() {
            let shift = ctx.line_shifts.get(li).copied().unwrap_or(ctx.shift);
            // Line top in paged space; only lines on this page are painted.
            let top = cy + line.top + shift;
            if let Some((w0, w1)) = ctx.window {
                if top >= w1 - 0.01 || top + line.height <= w0 + 0.01 {
                    continue;
                }
            }
            paint_line(
                line,
                prepared,
                runs,
                style.align,
                cx + ctx.dx,
                top - line.top + ctx.dy,
                cw,
                self.fonts,
                self.numbers,
                ctx.opacity,
                ops,
            );
        }
    }
}

/// One output glyph sequence in a single run/font.
struct Piece {
    run: u16,
    font: u16,
    glyphs: Vec<TextGlyph>,
    /// Pen advance of the whole piece.
    width: f32,
}

#[allow(clippy::too_many_arguments)]
fn paint_line(
    line: &Line,
    p: &Prepared,
    runs: &[TextRun],
    align: TextAlign,
    cx: f32,
    cy: f32,
    cw: f32,
    fonts: &FontSet<'_>,
    numbers: PageNumbers,
    opacity: f32,
    ops: &mut Vec<Op>,
) {
    let (s, e) = (line.start as usize, line.end as usize);

    // Justification: spread the free space over the spaces of the line.
    let spaces = p.glyphs[s..e]
        .iter()
        .filter(|g| g.flags & GLYPH_SPACE != 0)
        .count();
    let justify_extra =
        if align == TextAlign::Justify && !line.forced && spaces > 0 && line.ellipsis.is_none() {
            ((cw - line.width) / spaces as f32).max(0.0)
        } else {
            0.0
        };

    // Collect pieces per segment slice, substituting placeholders.
    let mut pieces: Vec<Piece> = Vec::new();
    let mut width = 0.0f32;
    for seg in &p.segments {
        let (gs, ge) = (seg.glyphs.start as usize, seg.glyphs.end as usize);
        let (a, b) = (gs.max(s), ge.min(e));
        if a >= b {
            continue;
        }
        let run = &runs[seg.run as usize];
        let mut piece = Piece {
            run: seg.run,
            font: seg.font,
            glyphs: Vec::with_capacity(b - a),
            width: 0.0,
        };
        if let Some(ph) = run.placeholder {
            let n = match ph {
                Placeholder::PageNumber => numbers.current,
                Placeholder::TotalPages => numbers.total,
            };
            let font = fonts.get(seg.font);
            let scale = font.scale(run.size);
            let ls = run.letter_spacing.unwrap_or(0.0);
            for c in n.to_string().chars() {
                let gid = font.glyph(c).unwrap_or(0);
                piece.glyphs.push(TextGlyph {
                    id: gid,
                    x: piece.width,
                    y_offset: 0.0,
                });
                piece.width += font.advance(gid) * scale + ls;
            }
        } else {
            for g in &p.glyphs[a..b] {
                if g.flags & GLYPH_INVISIBLE == 0 {
                    piece.glyphs.push(TextGlyph {
                        id: g.id,
                        x: piece.width + g.x_offset,
                        y_offset: g.y_offset,
                    });
                }
                piece.width += g.advance;
                if g.flags & GLYPH_SPACE != 0 {
                    piece.width += justify_extra;
                }
            }
        }
        width += piece.width;
        pieces.push(piece);
    }
    for extra in [line.hyphen, line.ellipsis].into_iter().flatten() {
        let piece = extra_piece(&extra);
        width += piece.width;
        pieces.push(piece);
    }

    // Placeholders may change the width: align with the painted width.
    let free = cw - width;
    let mut x = cx
        + match align {
            TextAlign::Left | TextAlign::Justify => 0.0,
            TextAlign::Center => free / 2.0,
            TextAlign::Right => free,
        };
    let baseline = cy + line.top + line.ascent;

    for piece in pieces {
        let run = &runs[piece.run as usize];
        let color = run.color.unwrap_or(BLACK);
        let alpha = color[3] * opacity;
        let piece_width = piece.width;
        if alpha < 1.0 {
            ops.push(Op::Save);
            ops.push(Op::SetOpacity(alpha));
        }
        if !piece.glyphs.is_empty() && color[3] > 0.0 {
            ops.push(Op::Text(TextOp {
                font: piece.font,
                size: run.size,
                color,
                x,
                y: baseline,
                glyphs: piece.glyphs,
            }));
        }
        if let Some(dec) = run.decoration.filter(|d| *d != Decoration::None) {
            let m = &fonts.get(piece.font).data.metrics;
            let scale = run.size / m.units_per_em;
            let (pos, thick) = match dec {
                Decoration::Underline => (m.underline_position, m.underline_thickness),
                _ => (m.strikeout_position, m.strikeout_thickness),
            };
            let thick = (thick * scale).max(0.25);
            // OpenType gives the top of the stroke, y up from the baseline.
            let y = baseline - pos * scale;
            ops.push(Op::FillRect {
                rect: Rect::new(x, y, piece_width, thick),
                color,
            });
        }
        if alpha < 1.0 {
            ops.push(Op::Restore);
        }
        x += piece_width;
    }
}

fn extra_piece(e: &Ellipsis) -> Piece {
    let glyphs = (0..e.count)
        .map(|i| TextGlyph {
            id: e.glyph,
            x: e.advance * f32::from(i),
            y_offset: 0.0,
        })
        .collect();
    Piece {
        run: e.run,
        font: e.font,
        glyphs,
        width: e.advance * f32::from(e.count),
    }
}
