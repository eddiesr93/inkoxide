//! Text nodes → `Op::Text` (plus decorations), with alignment,
//! justification and page-number placeholders.

use super::objects::InlineObject;
use super::optimal::SHRINK;
use super::prepare::number_glyphs;
use super::{
    baseline_shift, ColorGlyphs, ExtraGlyph, Line, PageText, Para, TextCache, TextShaper, WIDTH_EPS,
};
use crate::fonts::{FontKey, FontSet};
use crate::geom::Rect;
use crate::images::{ImageKey, ImagePlacement, ImageSet};
use crate::layout::{Geometry, LineBox, LineBoxes};
use crate::paint::{
    alt_span, span, ContentCtx, ContentPainter, Dash, DebugLine, InvisibleGlyph, Op, Path, PathSeg,
    SpaceAt, TextGlyph, TextOp, NO_RADII,
};
use crate::schema::{
    Color, Decoration, DecorationStyle, Node, NodeKind, Placeholder, RunAlign, TextAlign, TextRun,
    TextWrap, BLACK,
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
    /// The images of inline objects and colour glyphs.
    pub images: &'f ImageSet,
    pub colors: &'f ColorGlyphs,
    pub numbers: PageNumbers,
    /// The page's area, in page coordinates: decorations are drawn only
    /// where the page shows them.
    pub page_box: Rect,
}

/// What a line's pieces are drawn with besides their glyphs: page
/// numbers, the images of inline objects and colour glyphs, and the area
/// of the page, in the coordinates the text is drawn in (`None`: not
/// known, nothing is left out).
#[derive(Clone, Copy)]
struct Ink<'x> {
    numbers: PageNumbers,
    images: &'x ImageSet,
    colors: &'x ColorGlyphs,
    view: Option<Rect>,
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
        let ink = Ink {
            numbers: self.numbers,
            images: self.images,
            colors: self.colors,
            view: view(self.page_box, ctx.ctm),
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
            let b = layout.boxes.get(li).copied().unwrap_or(LineBox {
                left: 0.0,
                width: cw,
            });
            let content = Rect::new(cx + ctx.dx + b.left, top - line.top + ctx.dy, b.width, ch);
            let first_op = ops.len();
            let painted = paint_line(line, &para, content, ink, ctx.opacity, ops);
            // A line taller than the page is sliced across pages; its notes
            // are on the page it starts on.
            if ctx.window.is_some_and(|w| !w.starts(top)) {
                let line_ops = ops.split_off(first_op);
                ops.extend(line_ops.into_iter().filter(|op| !matches!(op, Op::Icon(_))));
            }
            if let Some(lines) = outline.as_deref_mut() {
                lines.push(painted.outline(line));
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
struct Piece<'t> {
    run: u16,
    font: u16,
    object: Option<u32>,
    glyphs: Vec<TextGlyph>,
    /// The text of each glyph, for the spans of those drawn as images: set
    /// in colour fonts only.
    texts: Vec<Option<&'t str>>,
    /// Glyphs the piece stands for, drawn or not.
    count: usize,
    /// Pen advance of the whole piece.
    width: f32,
}

/// Where a line was painted: its baseline, and each piece's run and box,
/// in visual order.
struct PaintedLine {
    top: f32,
    baseline: f32,
    pieces: Vec<(u16, Rect)>,
}

impl PaintedLine {
    /// The line's `debug` outline: its painted extent, and one box per
    /// stretch of a run, around what its pieces draw.
    fn outline(&self, line: &Line) -> DebugLine {
        let mut runs: Vec<(Rect, u16)> = Vec::new();
        for &(run, rect) in &self.pieces {
            match runs.last_mut() {
                // Pieces of one run (fonts, bidi levels, inline objects)
                // abut: one box around them.
                Some((r, last)) if *last == run && (r.x + r.w - rect.x).abs() < WIDTH_EPS => {
                    let top = r.y.min(rect.y);
                    let bottom = (r.y + r.h).max(rect.y + rect.h);
                    *r = Rect::new(r.x, top, rect.x + rect.w - r.x, bottom - top);
                }
                _ => runs.push((rect, run)),
            }
        }
        let x0 = self.pieces.first().map_or(0.0, |p| p.1.x);
        let x1 = self.pieces.last().map_or(x0, |p| p.1.x + p.1.w);
        DebugLine {
            rect: Rect::new(x0, self.top, x1 - x0, line.height),
            baseline: self.baseline,
            runs,
        }
    }
}

/// The bounds of page area `page` in the coordinates that `ctm` maps to
/// the page's (`None`: identity); `None` when `ctm` has no inverse.
fn view(page: Rect, ctm: Option<[f32; 6]>) -> Option<Rect> {
    let Some(m) = ctm else {
        return Some(page);
    };
    let inv = crate::geom::invert(m)?;
    let (x1, y1) = (page.x + page.w, page.y + page.h);
    let corners = [(page.x, page.y), (x1, page.y), (page.x, y1), (x1, y1)];
    let (mut lo, mut hi) = (
        (f32::INFINITY, f32::INFINITY),
        (f32::NEG_INFINITY, f32::NEG_INFINITY),
    );
    for c in corners {
        let (x, y) = crate::geom::apply(inv, c);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    let r = Rect::new(lo.0, lo.1, hi.0 - lo.0, hi.1 - lo.1);
    [r.x, r.y, r.w, r.h]
        .iter()
        .all(|v| v.is_finite())
        .then_some(r)
}

/// Height of run `r`'s glyph box above and below the line's baseline:
/// its font's ascender and descender at its size, around its own
/// (shifted) baseline.
fn run_extent(r: &TextRun, fonts: &FontSet<'_>) -> (f32, f32) {
    let shift = baseline_shift(r, fonts);
    let Some(key) = fonts.key(&r.font) else {
        return (r.size + shift, -shift);
    };
    let font = fonts.get(key);
    let m = font.metrics();
    let scale = font.scale(r.size);
    (m.ascender * scale + shift, -m.descender * scale - shift)
}

/// Paints one line of `para`. `content` is the paragraph's content box on
/// the page, placed for this line (a split paragraph shifts its lines).
fn paint_line(
    line: &Line,
    para: &Para<'_>,
    content: Rect,
    ink: Ink<'_>,
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
    let space_count = p.glyphs[s..e].iter().filter(|g| g.is_space()).count();
    let justify_extra = if align == TextAlign::Justify
        && !line.forced
        && space_count > 0
        && line.ellipsis.is_none()
    {
        // `pretty` lines may be set a little too wide, for their spaces
        // to shrink by up to a third.
        let least = if style.text_wrap == TextWrap::Pretty {
            let space: f32 = p.glyphs[s..e]
                .iter()
                .filter(|g| g.is_space())
                .map(|g| g.advance)
                .sum();
            -space * SHRINK / space_count as f32
        } else {
            0.0
        };
        ((cw - line.width) / space_count as f32).max(least)
    } else {
        0.0
    };

    // Segment slices of the line, in visual order. Segments are in glyph
    // order: those on the line follow the first that ends after its start.
    let from = p
        .segments
        .partition_point(|seg| seg.glyphs.end as usize <= s);
    let mut items: Vec<(&super::Segment, usize, usize)> = p.segments[from..]
        .iter()
        .take_while(|seg| (seg.glyphs.start as usize) < e)
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
        let color = fonts.get(seg.font).has_color();
        let mut piece = Piece {
            run: seg.run,
            font: seg.font,
            object: seg.object,
            glyphs: Vec::with_capacity(b - a),
            texts: Vec::new(),
            count: b - a,
            width: 0.0,
        };
        if let Some(ph) = run.placeholder.filter(|_| p.placeholders) {
            let n = ink.numbers.of(ph);
            for (id, advance) in number_glyphs(fonts.get(seg.font), run, n) {
                piece.glyphs.push(TextGlyph {
                    id,
                    x: piece.width,
                    y_offset: 0.0,
                });
                piece.width += advance;
            }
            if color {
                let digits = n.to_string().into_bytes();
                piece.texts = digits
                    .iter()
                    .map(|d| DIGITS.get(usize::from(d - b'0')).copied())
                    .collect();
            }
            piece.count = piece.glyphs.len();
        } else {
            let visual: Box<dyn Iterator<Item = usize>> = if seg.level % 2 == 1 {
                Box::new((a..b).rev())
            } else {
                Box::new(a..b)
            };
            for k in visual {
                let g = &p.glyphs[k];
                if !g.is_invisible() && !g.is_object() {
                    piece.glyphs.push(TextGlyph {
                        id: g.id,
                        x: piece.width + g.x_offset,
                        y_offset: g.y_offset,
                    });
                    if color {
                        piece.texts.push(p.glyph_text(seg, k));
                    }
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
    let extras = [(line.hyphen, "-"), (line.ellipsis, "\u{2026}")];
    for (extra, text) in extras.into_iter().filter_map(|(e, t)| Some((e?, t))) {
        let piece = extra_piece(&extra, text);
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
        let run = &runs[piece.run as usize];
        if let Some(note) = run.note.as_deref() {
            // It takes no room: its icon hangs from the line's top.
            crate::notes::paint_at(note, (x, cy + line.top), ops);
            continue;
        }
        if let Some(a) = run.attachment.as_deref() {
            crate::notes::paint_attachment_at(a, (x, cy + line.top), ops);
            continue;
        }
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
            let rect = object_rect(object, run, fonts, (x, baseline), line_box);
            painted.pieces.push((piece.run, rect));
            paint_object(object, rect, run, fonts, ink.images, baseline, ops);
            x += piece.width;
            continue;
        }
        let (above, below) = run_extent(run, fonts);
        let rect = Rect::new(x, baseline - above, piece.width, above + below);
        painted.pieces.push((piece.run, rect));
        let color = run.color.unwrap_or(BLACK);
        let alpha = color[3] * opacity;
        // The run's own baseline: raised or lowered by `verticalAlign`.
        let baseline = baseline - baseline_shift(run, fonts);
        let piece_width = piece.width;
        let piece_count = piece.count;
        if alpha < 1.0 {
            ops.push(Op::Save);
            ops.push(Op::SetOpacity(alpha));
        }
        if !piece.glyphs.is_empty() && color[3] > 0.0 {
            let text = TextOp {
                font: piece.font,
                size: run.size,
                color,
                x,
                y: baseline,
                glyphs: piece.glyphs,
            };
            // Colour glyphs become images, in spans with a space.
            let strut = if fonts.get(piece.font).has_color() {
                space_font(fonts, run)
            } else {
                None
            };
            ink.colors
                .push(text, strut, fonts, ink.images, &piece.texts, ops);
        }
        if let Some(dec) = run.decoration.filter(|d| *d != Decoration::None) {
            let font = fonts.get(piece.font);
            let m = font.metrics();
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
            let bounds = WaveBounds {
                view: ink.view,
                most: WAVES_PER_GLYPH * piece_count.max(1),
            };
            decoration(ops, style, rect, dcolor, (opacity, alpha), bounds);
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

/// Where inline object `object` of run `run` is drawn at `x` on a line
/// with its baseline at `baseline` and its box `(top, height)`.
fn object_rect(
    object: &InlineObject,
    run: &TextRun,
    fonts: &FontSet<'_>,
    (x, baseline): (f32, f32),
    (top, height): (f32, f32),
) -> Rect {
    let y = match object.extent(run, fonts) {
        Some((above, _)) => baseline - above,
        None if object.align == RunAlign::Top => top,
        None => top + height - object.height,
    };
    Rect::new(x, y, object.width, object.height)
}

/// Draws inline object `object` of run `run` in `rect`, on a line with its
/// baseline at `baseline`, in a span with its alternative text when it has
/// one.
fn paint_object(
    object: &InlineObject,
    rect: Rect,
    run: &TextRun,
    fonts: &FontSet<'_>,
    images: &ImageSet,
    baseline: f32,
    ops: &mut Vec<Op>,
) {
    let space = SpaceAt {
        font: fonts.key(&run.font),
        size: run.size,
        at: (rect.x, baseline),
        width: object.width,
    };
    alt_span(ops, fonts, object.alt.as_deref(), space, |ops| {
        images.paint_in(object.key, rect, object.placement, &NO_RADII, ops);
    });
}

/// The font of the invisible space in the span of an image drawn in run
/// `run` (a colour glyph): the run's own font, or another
/// ([`InvisibleGlyph::space_font`]).
fn space_font(fonts: &FontSet<'_>, run: &TextRun) -> Option<FontKey> {
    InvisibleGlyph::space_font(fonts, fonts.key(&run.font))
}

/// Draws image `key` in `rect` as `placement` says. With `text`, inside a
/// span with that text, along with its invisible glyph (laid over the
/// image, for extractors that read a span's text only off a glyph).
pub(super) fn image_span(
    images: &ImageSet,
    key: ImageKey,
    rect: Rect,
    placement: ImagePlacement,
    text: Option<(&str, Option<InvisibleGlyph>)>,
    ops: &mut Vec<Op>,
) {
    let draw = |ops: &mut Vec<Op>| images.paint_in(key, rect, placement, &NO_RADII, ops);
    match text {
        Some((text, glyph)) => span(ops, text, glyph, draw),
        None => draw(ops),
    }
}

/// The text of each digit, for the spans of colour digits.
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// The piece of extra glyph `e`, whose text is `text`.
fn extra_piece(e: &ExtraGlyph, text: &'static str) -> Piece<'static> {
    // Three dots stand for an ellipsis in a font without one.
    let text = if e.count > 1 { "." } else { text };
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
        texts: vec![Some(text); usize::from(e.count)],
        count: usize::from(e.count),
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

/// Most half waves of a `wavy` decoration per glyph it is under: wider
/// pieces (wide letter spacing, a thin decoration under large text) get
/// longer waves.
pub(crate) const WAVES_PER_GLYPH: usize = 16;

/// Where a `wavy` decoration may draw its waves.
#[derive(Clone, Copy)]
struct WaveBounds {
    /// The area the page shows, in the decoration's coordinates (`None`:
    /// anywhere).
    view: Option<Rect>,
    /// The most half waves.
    most: usize,
}

/// Draws a text decoration in `rect`: its top edge, length and thickness.
/// `opacity` is the cumulative opacity, `alpha` the opacity in effect for
/// the run's colour; the decoration colour's own alpha replaces the run's.
fn decoration(
    ops: &mut Vec<Op>,
    style: DecorationStyle,
    rect: Rect,
    color: Color,
    (opacity, alpha): (f32, f32),
    bounds: WaveBounds,
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
            if let Some(path) = wave(x, mid, w, thick, bounds) {
                stroke(ops, path, None);
            }
        }
    }
    if swap {
        ops.push(Op::Restore);
    }
}

/// The path of a `wavy` decoration `w` long from `(x, mid)`, `thick`
/// thick: half waves of width 2·amplitude, as cubic arcs, only where the
/// page shows them (`None`: nowhere).
fn wave(x: f32, mid: f32, w: f32, thick: f32, bounds: WaveBounds) -> Option<Path> {
    let amp = (thick * 1.5).max(0.75);
    let mut half = 2.0 * amp;
    let end = x + w;
    // The half waves the page shows, with a half wave and the stroke to
    // spare on each side.
    let (mut from, mut to) = (0.0f32, w);
    if let Some(v) = bounds.view {
        let spare = half + thick;
        let reach = amp * 1.33 + thick;
        if mid + reach < v.y || mid - reach > v.y + v.h {
            return None;
        }
        from = ((v.x - spare - x) / half).floor().max(0.0) * half;
        to = to.min(v.x + v.w + spare - x);
    }
    if to - from <= WIDTH_EPS {
        return None;
    }
    // Waves from `from`, so that the part shown is the part a whole wave
    // shows; past the most, longer ones.
    let up_first = (from / half).round() % 2.0 == 0.0;
    let count = ((to - from) / half).ceil();
    let most = bounds.most as f32;
    if count > most {
        half = (to - from) / most;
    }
    let mut cx = x + from;
    let stop = (x + to).min(end);
    let mut segs = vec![PathSeg::MoveTo(cx, mid)];
    let mut up = up_first;
    for _ in 0..=bounds.most {
        if cx >= stop - WIDTH_EPS {
            break;
        }
        let nx = (cx + half).min(end);
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
    Some(Path { segs })
}

#[cfg(test)]
mod tests {
    use super::{reorder, wave, WaveBounds};
    use crate::geom::Rect;
    use crate::paint::PathSeg;

    fn ends(segs: &[PathSeg]) -> Vec<(f32, f32)> {
        segs.iter()
            .map(|s| match *s {
                PathSeg::MoveTo(x, y)
                | PathSeg::LineTo(x, y)
                | PathSeg::CubicTo(_, _, _, _, x, y) => (x, y),
                PathSeg::Close => panic!("closed wave"),
            })
            .collect()
    }

    #[test]
    fn a_wave_is_drawn_where_the_page_shows_it() {
        let all = WaveBounds {
            view: None,
            most: 10_000,
        };
        let full = wave(0.0, 10.0, 1000.0, 0.5, all).unwrap().segs;
        let view = Some(Rect::new(400.0, 0.0, 100.0, 20.0));
        let shown = wave(0.0, 10.0, 1000.0, 0.5, WaveBounds { view, ..all })
            .unwrap()
            .segs;
        assert!(shown.len() < full.len() / 5, "{} curves", shown.len());
        // The same curves as the whole wave's, from before the view to
        // after it.
        let (PathSeg::MoveTo(x0, _), PathSeg::CubicTo(_, y1, ..)) = (shown[0], shown[1]) else {
            panic!("not a wave");
        };
        let k = full
            .iter()
            .position(|s| matches!(*s, PathSeg::CubicTo(.., x, _) if (x - x0).abs() < 1e-3))
            .unwrap();
        let PathSeg::CubicTo(_, y, ..) = full[k + 1] else {
            panic!("not a wave");
        };
        assert!((y - y1).abs() < 1e-3);
        for (a, b) in ends(&shown[1..]).iter().zip(ends(&full[k + 1..])) {
            assert!((a.0 - b.0).abs() < 1e-2 && (a.1 - b.1).abs() < 1e-3);
        }
        let end = ends(&shown).last().unwrap().0;
        assert!(x0 < 400.0 && end > 500.0, "{x0}..{end}");
        // Above the page: nothing.
        let view = Some(Rect::new(0.0, 100.0, 1000.0, 20.0));
        assert!(wave(0.0, 10.0, 1000.0, 0.5, WaveBounds { view, ..all }).is_none());
    }

    #[test]
    fn a_wave_has_at_most_its_half_waves() {
        let bounds = WaveBounds {
            view: None,
            most: 16,
        };
        let segs = wave(0.0, 10.0, 100_000.0, 0.1, bounds).unwrap().segs;
        assert!(segs.len() <= 18, "{} curves", segs.len());
        assert!((ends(&segs).last().unwrap().0 - 100_000.0).abs() < 1.0);
    }

    #[test]
    fn reorder_reverses_rtl_runs() {
        // "abc ARABIC 12" in an LTR paragraph: levels 0 0 1 1 2.
        let mut items = vec![(0, 'a'), (0, 'b'), (1, 'X'), (1, 'Y'), (2, '1')];
        reorder(&mut items, |i| i.0);
        let order: String = items.iter().map(|i| i.1).collect();
        assert_eq!(order, "ab1YX");
    }
}
