//! Text nodes → `Op::Text` (plus decorations), with alignment,
//! justification and page-number placeholders.

use super::optimal::SHRINK;
use super::prepare::number_glyphs;
use super::{baseline_shift, ExtraGlyph, Line, PageText, Para, TextCache, TextShaper, WIDTH_EPS};
use crate::fonts::FontSet;
use crate::images::ImageSet;
use crate::layout::{Geometry, LineBoxes};
use crate::paint::{
    span, ContentCtx, ContentPainter, Dash, DebugLine, InvisibleGlyph, Op, Path, PathSeg, Rect,
    TextGlyph, TextOp, NO_RADII,
};
use crate::schema::{
    Color, Decoration, DecorationStyle, Node, NodeKind, Placeholder, TextAlign, TextWrap, BLACK,
};

/// Page numbers substituted for placeholders on the page being painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageNumbers {
    pub current: u32,
    pub total: u32,
}

impl PageNumbers {
    /// The number placeholder `ph` shows.
    pub fn of(self, ph: Placeholder) -> u32 {
        match ph {
            Placeholder::PageNumber => self.current,
            Placeholder::TotalPages => self.total,
        }
    }
}

pub(crate) struct TextPainter<'c, 's, 'f, 'a, S: TextShaper> {
    pub cache: &'c TextCache<'s, S>,
    /// The page's own layouts of paragraphs with placeholders, when
    /// painting a header, footer or overlay laid out for this page.
    pub page: Option<&'c PageText<'c, 's, S>>,
    pub fonts: &'f FontSet<'a>,
    /// The images of inline objects.
    pub images: &'f ImageSet,
    pub numbers: PageNumbers,
}

impl<S: TextShaper> TextPainter<'_, '_, '_, '_, S> {
    /// Paints text node `node`; with `outline`, records its lines there.
    fn paint_text(
        &self,
        node: &Node,
        geo: &Geometry,
        ctx: &ContentCtx<'_>,
        ops: &mut Vec<Op>,
        mut outline: Option<&mut Vec<DebugLine>>,
    ) {
        let NodeKind::Text { runs } = &node.kind else {
            return;
        };
        let Rect {
            x: cx,
            y: cy,
            w: cw,
            h: ch,
        } = geo.content_box();
        let plain;
        let boxes = if let Some(b) = ctx.line_boxes {
            b
        } else {
            plain = LineBoxes::plain(cw);
            &plain
        };
        let paragraph = self
            .page
            .and_then(|page| page.paragraph(node, boxes))
            .or_else(|| self.cache.paragraph(node, boxes));
        let Some((layout, prepared, style)) = paragraph else {
            return;
        };
        let para = Para {
            prepared,
            runs,
            style,
            fonts: self.fonts,
        };
        for (li, line) in layout.lines.iter().enumerate() {
            let shift = ctx.line_shifts.get(li).copied().unwrap_or(ctx.shift);
            // Line top in paged space; only lines on this page are painted.
            let top = cy + line.top + shift;
            if ctx.window.is_some_and(|w| w.hides(top, line.height)) {
                continue;
            }
            // The line's box: the content box, or beside a float, the part
            // of it the line was broken in.
            let (bx, bw) = layout.boxes.get(li).copied().unwrap_or((0.0, cw));
            let content = Rect::new(cx + ctx.dx + bx, top - line.top + ctx.dy, bw, ch);
            let painted = paint_line(
                line,
                &para,
                content,
                (self.numbers, self.images),
                ctx.opacity,
                ops,
            );
            if let Some(lines) = outline.as_deref_mut() {
                lines.push(painted.outline(line, &para));
            }
        }
    }
}

impl<S: TextShaper> ContentPainter for TextPainter<'_, '_, '_, '_, S> {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
        self.paint_text(node, geo, ctx, ops, None);
    }

    fn paint_outlined(
        &mut self,
        node: &Node,
        geo: &Geometry,
        ctx: &ContentCtx<'_>,
        ops: &mut Vec<Op>,
        lines: &mut Vec<DebugLine>,
    ) {
        self.paint_text(node, geo, ctx, ops, Some(lines));
    }

    fn numbers(&self) -> PageNumbers {
        self.numbers
    }
}

/// One output glyph sequence in a single run/font, or an inline object.
struct Piece {
    run: u16,
    font: u16,
    object: Option<u32>,
    glyphs: Vec<TextGlyph>,
    /// Pen advance of the whole piece.
    width: f32,
}

/// Where a line was painted: its baseline, and each piece's run, start
/// and advance, in visual order.
struct PaintedLine {
    top: f32,
    baseline: f32,
    pieces: Vec<(u16, f32, f32)>,
}

impl PaintedLine {
    /// The line's `debug` outline: its painted extent, and one box per
    /// stretch of a run, from its font's ascender to its descender.
    fn outline(&self, line: &Line, para: &Para<'_>) -> DebugLine {
        let mut runs: Vec<(Rect, u16)> = Vec::new();
        for &(run, x, w) in &self.pieces {
            match runs.last_mut() {
                // Pieces of one run (fonts, bidi levels) abut: one box.
                Some((r, last)) if *last == run && (r.x + r.w - x).abs() < WIDTH_EPS => {
                    r.w = x + w - r.x;
                }
                _ => {
                    let (above, below) = run_extent(para, run);
                    let rect = Rect::new(x, self.baseline - above, w, above + below);
                    runs.push((rect, run));
                }
            }
        }
        let x0 = self.pieces.first().map_or(0.0, |p| p.1);
        let x1 = self.pieces.last().map_or(x0, |p| p.1 + p.2);
        DebugLine {
            rect: Rect::new(x0, self.top, x1 - x0, line.height),
            baseline: self.baseline,
            runs,
        }
    }
}

/// Height of run `run`'s glyph box above and below the line's baseline:
/// its font's ascender and descender at its size, around its own
/// (shifted) baseline.
fn run_extent(para: &Para<'_>, run: u16) -> (f32, f32) {
    let r = &para.runs[usize::from(run)];
    let shift = baseline_shift(r, para.fonts);
    let Some(key) = para.fonts.key(&r.font) else {
        return (r.size + shift, -shift);
    };
    let font = para.fonts.get(key);
    let m = &font.data.metrics;
    let scale = font.scale(r.size);
    (m.ascender * scale + shift, -m.descender * scale - shift)
}

/// Paints one line of `para`. `content` is the paragraph's content box on
/// the page, placed for this line (a split paragraph shifts its lines).
fn paint_line(
    line: &Line,
    para: &Para<'_>,
    content: Rect,
    (numbers, images): (PageNumbers, &ImageSet),
    opacity: f32,
    ops: &mut Vec<Op>,
) -> PaintedLine {
    let Para {
        prepared: p,
        runs,
        style,
        fonts,
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
    let align = match style.align {
        TextAlign::Start if rtl => TextAlign::Right,
        TextAlign::Start => TextAlign::Left,
        TextAlign::End if rtl => TextAlign::Left,
        TextAlign::End => TextAlign::Right,
        a => a,
    };

    // Justification: spread the free space over the spaces of the line.
    let spaces = p.glyphs[s..e].iter().filter(|g| g.is_space()).count();
    let justify_extra =
        if align == TextAlign::Justify && !line.forced && spaces > 0 && line.ellipsis.is_none() {
            // `pretty` lines may be set a little too wide, for their spaces
            // to shrink by up to a third.
            let least = if style.text_wrap == TextWrap::Pretty {
                let space: f32 = p.glyphs[s..e]
                    .iter()
                    .filter(|g| g.is_space())
                    .map(|g| g.advance)
                    .sum();
                -space * SHRINK / spaces as f32
            } else {
                0.0
            };
            ((cw - line.width) / spaces as f32).max(least)
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
            object: seg.object,
            glyphs: Vec::with_capacity(b - a),
            width: 0.0,
        };
        if let Some(ph) = run.placeholder.filter(|_| p.placeholders) {
            for (id, advance) in number_glyphs(fonts.get(seg.font), run, numbers.of(ph)) {
                piece.glyphs.push(TextGlyph {
                    id,
                    x: piece.width,
                    y_offset: 0.0,
                });
                piece.width += advance;
            }
        } else {
            let slice = &p.glyphs[a..b];
            let visual: Box<dyn Iterator<Item = &super::Glyph>> = if seg.level % 2 == 1 {
                Box::new(slice.iter().rev())
            } else {
                Box::new(slice.iter())
            };
            for g in visual {
                if !g.is_invisible() && !g.is_object() {
                    piece.glyphs.push(TextGlyph {
                        id: g.id,
                        x: piece.width + g.x_offset,
                        y_offset: g.y_offset,
                    });
                }
                piece.width += g.advance;
                if g.is_space() {
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
    // Link areas of the line, adjacent pieces of one target merged
    // (adjacent pieces abut exactly).
    let mut links: Vec<(f32, f32, &str)> = Vec::new();
    let mut painted = PaintedLine {
        top: cy + line.top,
        baseline,
        pieces: Vec::with_capacity(pieces.len()),
    };

    for piece in pieces {
        painted.pieces.push((piece.run, x, piece.width));
        let run = &runs[piece.run as usize];
        if let Some(target) = run.link.as_deref() {
            match links.last_mut() {
                Some((_, x1, t)) if *t == target && (*x1 - x).abs() < WIDTH_EPS => {
                    *x1 = x + piece.width;
                }
                _ => links.push((x, x + piece.width, target)),
            }
        }
        if let Some(o) = piece.object {
            // An image: no colour of its own, no decorations.
            let object = &p.objects[o as usize];
            let line_box = (cy + line.top, line.height);
            paint_object(object, run, fonts, images, (x, baseline), line_box, ops);
            x += piece.width;
            continue;
        }
        let color = run.color.unwrap_or(BLACK);
        let alpha = color[3] * opacity;
        // The run's own baseline: raised or lowered by `verticalAlign`.
        let baseline = baseline - baseline_shift(run, fonts);
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
            let font = fonts.get(piece.font);
            let m = &font.data.metrics;
            let scale = font.scale(run.size);
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
    painted
}

/// Draws inline object `object` of run `run` at `x` on a line with its
/// baseline at `baseline` and its box `(top, height)`, in a span with its
/// alternative text when it has one.
fn paint_object(
    object: &super::objects::InlineObject,
    run: &crate::schema::TextRun,
    fonts: &FontSet<'_>,
    images: &ImageSet,
    (x, baseline): (f32, f32),
    (top, height): (f32, f32),
    ops: &mut Vec<Op>,
) {
    let y = match object.extent(run, fonts) {
        Some((above, _)) => baseline - above,
        None if run.vertical_align == Some(crate::schema::RunAlign::Top) => top,
        None => top + height - object.height,
    };
    let rect = Rect::new(x, y, object.width, object.height);
    let draw = |ops: &mut Vec<Op>| {
        images.paint_in(
            object.key,
            rect,
            object.fit,
            object.position,
            &NO_RADII,
            ops,
        );
    };
    match object.alt.as_deref().filter(|a| !a.is_empty()) {
        Some(alt) => {
            let space = fonts.key(&run.font).and_then(|k| {
                InvisibleGlyph::space(fonts, k, run.size, (x, baseline), object.width)
            });
            span(ops, alt, space, draw);
        }
        None => draw(ops),
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
        object: None,
        glyphs,
        width: e.advance * f32::from(e.count),
    }
}

/// UAX #9 rule L2: from the highest level down to the lowest odd one,
/// reverses every maximal run of items at that level or higher.
pub(super) fn reorder<T>(items: &mut [T], level: impl Fn(&T) -> u8) {
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
            while cx < x + w - WIDTH_EPS {
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
