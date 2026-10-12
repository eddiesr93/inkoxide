//! Text nodes → `Op::Text` (plus decorations), with alignment,
//! justification and page-number placeholders.

use super::{
    ExtraGlyph, Line, PageText, Prepared, TextCache, TextShaper, GLYPH_INVISIBLE, GLYPH_SPACE,
};
use crate::fonts::FontSet;
use crate::layout::Geometry;
use crate::paint::{ContentCtx, ContentPainter, Dash, Op, Path, PathSeg, Rect, TextGlyph, TextOp};
use crate::schema::{
    Color, Decoration, DecorationStyle, Node, NodeKind, Placeholder, TextAlign, TextRun, BLACK,
};

/// Page numbers substituted for placeholders on the page being painted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageNumbers {
    pub current: u32,
    pub total: u32,
}

pub(crate) struct TextPainter<'c, 's, 'f, 'a, S: TextShaper> {
    pub cache: &'c TextCache<'s, S>,
    /// The page's own layouts of paragraphs with placeholders, when
    /// painting a header or footer laid out for this page.
    pub page: Option<&'c PageText<'c, 's, S>>,
    pub fonts: &'f FontSet<'a>,
    pub numbers: PageNumbers,
}

impl<S: TextShaper> ContentPainter for TextPainter<'_, '_, '_, '_, S> {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
        if let NodeKind::Field(f) = &node.kind {
            crate::forms::paint(f, geo, ctx, self.fonts, ops);
            return;
        }
        let NodeKind::Text { runs } = &node.kind else {
            return;
        };
        let Rect {
            x: cx,
            y: cy,
            w: cw,
            h: ch,
        } = geo.content_box();
        let paragraph = self
            .page
            .and_then(|page| page.paragraph(node, cw))
            .or_else(|| self.cache.paragraph(node, cw));
        let Some((layout, prepared, style)) = paragraph else {
            return;
        };
        let para = Paragraph {
            prepared,
            runs,
            align: style.align,
        };
        for (li, line) in layout.lines.iter().enumerate() {
            let shift = ctx.line_shifts.get(li).copied().unwrap_or(ctx.shift);
            // Line top in paged space; only lines on this page are painted.
            let top = cy + line.top + shift;
            if ctx.window.is_some_and(|w| w.hides(top, line.height)) {
                continue;
            }
            let content = Rect::new(cx + ctx.dx, top - line.top + ctx.dy, cw, ch);
            paint_line(
                line,
                &para,
                content,
                self.fonts,
                self.numbers,
                ctx.opacity,
                ops,
            );
        }
    }

    fn numbers(&self) -> PageNumbers {
        self.numbers
    }
}

/// What every line of a paragraph is painted with.
struct Paragraph<'p> {
    prepared: &'p Prepared,
    runs: &'p [TextRun],
    align: TextAlign,
}

/// One output glyph sequence in a single run/font.
struct Piece {
    run: u16,
    font: u16,
    glyphs: Vec<TextGlyph>,
    /// Pen advance of the whole piece.
    width: f32,
}

/// Paints one line of `para`. `content` is the paragraph's content box on
/// the page, placed for this line (a split paragraph shifts its lines).
fn paint_line(
    line: &Line,
    para: &Paragraph<'_>,
    content: Rect,
    fonts: &FontSet<'_>,
    numbers: PageNumbers,
    opacity: f32,
    ops: &mut Vec<Op>,
) {
    let Paragraph {
        prepared: p,
        runs,
        align,
    } = *para;
    let Rect {
        x: cx,
        y: cy,
        w: cw,
        ..
    } = content;
    let (s, e) = (line.start as usize, line.end as usize);
    let rtl = p.rtl;
    // The indent narrows the line box on its start side.
    let (cx, cw) = match (line.indent, rtl) {
        (0.0, _) => (cx, cw),
        (i, false) => (cx + i, cw - i),
        (i, true) => (cx, cw - i),
    };
    let align = match align {
        TextAlign::Start if rtl => TextAlign::Right,
        TextAlign::Start => TextAlign::Left,
        TextAlign::End if rtl => TextAlign::Left,
        TextAlign::End => TextAlign::Right,
        a => a,
    };

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

    // Segment slices of the line, in visual order.
    let mut items: Vec<(&super::Segment, usize, usize)> = p
        .segments
        .iter()
        .filter_map(|seg| {
            let (gs, ge) = (seg.glyphs.start as usize, seg.glyphs.end as usize);
            let (a, b) = (gs.max(s), ge.min(e));
            (a < b).then_some((seg, a, b))
        })
        .collect();
    if p.bidi {
        reorder(&mut items, |it| it.0.level);
    }

    // Collect pieces per segment slice, substituting placeholders.
    let mut pieces: Vec<Piece> = Vec::new();
    let mut width = 0.0f32;
    for (seg, a, b) in items {
        let run = &runs[seg.run as usize];
        let mut piece = Piece {
            run: seg.run,
            font: seg.font,
            glyphs: Vec::with_capacity(b - a),
            width: 0.0,
        };
        if let Some(ph) = run.placeholder.filter(|_| p.placeholders) {
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
            let slice = &p.glyphs[a..b];
            let visual: Box<dyn Iterator<Item = &super::Glyph>> = if seg.level % 2 == 1 {
                Box::new(slice.iter().rev())
            } else {
                Box::new(slice.iter())
            };
            for g in visual {
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
        if rtl {
            pieces.insert(0, piece);
        } else {
            pieces.push(piece);
        }
    }

    // Placeholders may change the width: align with the painted width.
    let free = cw - width;
    let mut x = cx
        + match align {
            TextAlign::Justify if rtl => free.max(0.0),
            TextAlign::Left | TextAlign::Justify | TextAlign::Start | TextAlign::End => 0.0,
            TextAlign::Center => free / 2.0,
            TextAlign::Right => free,
        };
    let baseline = cy + line.top + line.ascent;
    // Link areas of the line, adjacent pieces of one target merged.
    let mut links: Vec<(f32, f32, &str)> = Vec::new();

    for piece in pieces {
        let run = &runs[piece.run as usize];
        if let Some(target) = run.link.as_deref() {
            match links.last_mut() {
                Some((_, x1, t)) if *t == target && (*x1 - x).abs() < 0.01 => {
                    *x1 = x + piece.width;
                }
                _ => links.push((x, x + piece.width, target)),
            }
        }
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
            let thick = run
                .decoration_thickness
                .unwrap_or((thick * scale).max(0.25));
            // OpenType gives the top of the stroke, y up from the baseline.
            let y = baseline - pos * scale;
            let dcolor = run.decoration_color.unwrap_or(color);
            let style = run.decoration_style.unwrap_or(DecorationStyle::Solid);
            let rect = Rect::new(x, y, piece_width, thick);
            decoration(ops, style, rect, dcolor, opacity, alpha);
        }
        if alpha < 1.0 {
            ops.push(Op::Restore);
        }
        x += piece_width;
    }
    for (x0, x1, target) in links {
        if x1 > x0 {
            ops.push(Op::Link {
                rect: Rect::new(x0, cy + line.top, x1 - x0, line.height),
                target: target.into(),
            });
        }
    }
}

fn extra_piece(e: &ExtraGlyph) -> Piece {
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

/// UAX #9 rule L2: from the highest level down to the lowest odd one,
/// reverses every maximal run of items at that level or higher.
fn reorder<T>(items: &mut [T], level: impl Fn(&T) -> u8) {
    let Some(max) = items.iter().map(&level).max() else {
        return;
    };
    let Some(min_odd) = items.iter().map(&level).filter(|l| l % 2 == 1).min() else {
        return;
    };
    for lvl in (min_odd..=max).rev() {
        let mut i = 0;
        while i < items.len() {
            if level(&items[i]) >= lvl {
                let mut j = i + 1;
                while j < items.len() && level(&items[j]) >= lvl {
                    j += 1;
                }
                items[i..j].reverse();
                i = j;
            } else {
                i += 1;
            }
        }
    }
}

/// Draws a text decoration in `rect`: its top edge, length and thickness.
/// `alpha` is the opacity in effect for the run's colour; the decoration
/// colour's own alpha replaces the run's.
fn decoration(
    ops: &mut Vec<Op>,
    style: DecorationStyle,
    rect: Rect,
    color: Color,
    opacity: f32,
    alpha: f32,
) {
    let Rect { x, y, w, h: thick } = rect;
    let own = color[3] * opacity;
    let swap = (own - alpha).abs() > f32::EPSILON;
    if swap {
        ops.push(Op::Save);
        ops.push(Op::SetOpacity(own));
    }
    let mid = y + thick / 2.0;
    let stroke = |ops: &mut Vec<Op>, path: Path, dash: Option<Dash>| {
        ops.push(Op::StrokePath {
            path,
            color,
            width: thick,
            dash,
        });
    };
    let line = |y: f32| Path {
        segs: vec![PathSeg::MoveTo(x, y), PathSeg::LineTo(x + w, y)],
    };
    match style {
        DecorationStyle::Solid => ops.push(Op::FillRect { rect, color }),
        DecorationStyle::Double => {
            // Two strokes of the thickness, one thickness apart.
            ops.push(Op::FillRect { rect, color });
            ops.push(Op::FillRect {
                rect: Rect::new(x, y + 2.0 * thick, w, thick),
                color,
            });
        }
        DecorationStyle::Dotted => stroke(
            ops,
            line(mid),
            Some(Dash {
                pattern: [0.0, 2.0 * thick],
                round_cap: true,
            }),
        ),
        DecorationStyle::Dashed => stroke(
            ops,
            line(mid),
            Some(Dash {
                pattern: [3.0 * thick, 2.0 * thick],
                round_cap: false,
            }),
        ),
        DecorationStyle::Wavy => {
            // Half waves of width 2·amplitude, as cubic arcs.
            let amp = (thick * 1.5).max(0.75);
            let half = 2.0 * amp;
            let mut segs = vec![PathSeg::MoveTo(x, mid)];
            let mut cx = x;
            let mut up = true;
            while cx < x + w - 1e-3 {
                let nx = (cx + half).min(x + w);
                let k = (nx - cx) / half;
                let dy = if up { -amp } else { amp } * k;
                segs.push(PathSeg::CubicTo(
                    cx + (nx - cx) / 3.0,
                    mid + dy * 1.33,
                    cx + 2.0 * (nx - cx) / 3.0,
                    mid + dy * 1.33,
                    nx,
                    mid,
                ));
                cx = nx;
                up = !up;
            }
            stroke(ops, Path { segs }, None);
        }
    }
    if swap {
        ops.push(Op::Restore);
    }
}

#[cfg(test)]
mod tests {
    use super::reorder;

    #[test]
    fn reorder_reverses_rtl_runs() {
        // "abc ARABIC 12" in an LTR paragraph: levels 0 0 1 1 2.
        let mut items = vec![(0, 'a'), (0, 'b'), (1, 'X'), (1, 'Y'), (2, '1')];
        reorder(&mut items, |i| i.0);
        let order: String = items.iter().map(|i| i.1).collect();
        assert_eq!(order, "ab1YX");
    }
}
