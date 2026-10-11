//! `debug` outlines: the margin, border, padding and content boxes of an
//! element, and its size as a small label, drawn over the page.
//!
//! The box painter records a [`DebugBox`] for each element to outline as
//! it paints it (it knows the element's boxes on this page, split ones
//! included); the page painter draws them last, on top of everything.
//! Labels are drawn with lines, not text: they need no font and add
//! nothing to the document's text (copy, search, extraction).

use super::{FillRule, Op, Path, PathSeg, Rect};
use crate::schema::Color;

/// Margin: orange, like browser developer tools.
pub(crate) const MARGIN: Color = [0.96, 0.62, 0.26, 0.3];
/// Border: yellow.
pub(crate) const BORDER: Color = [1.0, 0.82, 0.3, 0.45];
/// Padding: green.
pub(crate) const PADDING: Color = [0.5, 0.75, 0.4, 0.35];
/// Content box outline: blue.
pub(crate) const CONTENT: Color = [0.2, 0.5, 0.9, 1.0];
const CONTENT_WIDTH: f32 = 0.5;
/// Label background (blue) and digits (white).
pub(crate) const LABEL_BG: Color = [0.16, 0.38, 0.75, 0.85];
const LABEL_FG: Color = [1.0, 1.0, 1.0, 1.0];

/// Glyph unit of the label digits, points: digits are 2 × 4 units.
const UNIT: f32 = 1.1;
const STROKE: f32 = 0.55;
/// Label padding around the digits, points.
const LABEL_PAD: f32 = 1.2;

/// One element to outline on a page, in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DebugBox {
    /// Border box as painted on this page (a split box: its painted
    /// height, sliced by the page clip).
    pub rect: Rect,
    /// `[top, right, bottom, left]`.
    pub border: [f32; 4],
    pub padding: [f32; 4],
    pub margin: [f32; 4],
    /// Laid-out border-box size, for the label.
    pub size: [f32; 2],
    /// Top and bottom of the part of the border box shown on this page,
    /// where its label goes.
    pub shown: (f32, f32),
    /// Transform in effect for the element (rotation, `transform`).
    pub ctm: Option<[f32; 6]>,
}

/// What to outline while painting one part of a page.
#[derive(Debug, Default)]
pub(crate) struct DebugBoxes {
    /// Outline every element (`debug` on the page).
    pub all: bool,
    pub boxes: Vec<DebugBox>,
}

impl DebugBoxes {
    pub fn new(all: bool) -> Self {
        Self {
            all,
            boxes: Vec::new(),
        }
    }
}

/// Draws the outlines of `boxes`. `labels` are the labels already on the
/// page: a label moves to another corner of its box rather than cover one.
pub(crate) fn draw(boxes: &[DebugBox], ops: &mut Vec<Op>, labels: &mut Vec<Rect>) {
    for b in boxes {
        if let Some(m) = b.ctm {
            ops.push(Op::Save);
            ops.push(Op::Transform(m));
        }
        draw_box(b, ops, labels);
        if b.ctm.is_some() {
            ops.push(Op::Restore);
        }
    }
}

/// The page's own boxes: its margins and content area, with its size.
pub(crate) fn page_box(width: f32, height: f32, margin: [f32; 4]) -> DebugBox {
    let [t, r, b, l] = margin;
    DebugBox {
        rect: Rect::new(l, t, width - l - r, height - t - b),
        border: [0.0; 4],
        padding: [0.0; 4],
        margin,
        size: [width, height],
        shown: (0.0, height),
        ctm: None,
    }
}

/// `r` grown by `[top, right, bottom, left]` (shrunk when negative).
fn outset(r: Rect, e: [f32; 4]) -> Rect {
    let [t, right, b, l] = e;
    Rect::new(
        r.x - l,
        r.y - t,
        (r.w + l + right).max(0.0),
        (r.h + t + b).max(0.0),
    )
}

fn rect_path(p: &mut Path, r: Rect) {
    p.segs.extend([
        PathSeg::MoveTo(r.x, r.y),
        PathSeg::LineTo(r.x + r.w, r.y),
        PathSeg::LineTo(r.x + r.w, r.y + r.h),
        PathSeg::LineTo(r.x, r.y + r.h),
        PathSeg::Close,
    ]);
}

/// Fills the area between `outer` and `inner`.
fn ring(outer: Rect, inner: Rect, color: Color, ops: &mut Vec<Op>) {
    let mut path = Path::default();
    rect_path(&mut path, outer);
    rect_path(&mut path, inner);
    with_alpha(color[3], ops, |ops| {
        ops.push(Op::FillPath {
            path,
            color,
            rule: FillRule::EvenOdd,
        });
    });
}

fn with_alpha(alpha: f32, ops: &mut Vec<Op>, f: impl FnOnce(&mut Vec<Op>)) {
    if alpha >= 1.0 {
        f(ops);
        return;
    }
    ops.push(Op::Save);
    ops.push(Op::SetOpacity(alpha));
    f(ops);
    ops.push(Op::Restore);
}

fn draw_box(b: &DebugBox, ops: &mut Vec<Op>, labels: &mut Vec<Rect>) {
    let border = b.rect;
    let padding = outset(border, b.border.map(|v| -v));
    let content = outset(padding, b.padding.map(|v| -v));
    let any = |e: [f32; 4]| e.iter().any(|v| *v != 0.0);
    if any(b.margin) {
        ring(outset(border, b.margin), border, MARGIN, ops);
    }
    if any(b.border) {
        ring(border, padding, BORDER, ops);
    }
    if any(b.padding) {
        ring(padding, content, PADDING, ops);
    }
    let mut path = Path::default();
    rect_path(&mut path, content);
    ops.push(Op::StrokePath {
        path,
        color: CONTENT,
        width: CONTENT_WIDTH,
        dash: None,
    });
    let text = size_text(b.size);
    let (lw, lh) = label_size(&text);
    let (top, bottom) = b.shown;
    let (left, right) = (border.x, border.x + border.w - lw);
    let corners = [
        (left, top),
        (right, top),
        (left, bottom - lh),
        (right, bottom - lh),
    ];
    let at = |(x, y): (f32, f32)| Rect::new(x, y, lw, lh);
    let place = if b.ctm.is_some() {
        // Rotated or transformed: its own corner, wherever that lands.
        corners[0]
    } else {
        let free = |r: Rect| !labels.iter().any(|l| overlap(*l, r));
        let c = corners
            .into_iter()
            .find(|&c| free(at(c)))
            .unwrap_or(corners[0]);
        labels.push(at(c));
        c
    };
    label(place.0, place.1, &text, ops);
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// `120×48.5`: a size in points, to two decimals at most.
fn size_text(size: [f32; 2]) -> String {
    let num = |v: f32| {
        let s = format!("{:.2}", v.max(0.0));
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_owned()
    };
    format!("{}×{}", num(size[0]), num(size[1]))
}

/// Strokes of a label character on a 2 × 4 grid (y down), and its width.
fn glyph(c: char) -> (&'static [&'static [(f32, f32)]], f32) {
    const ZERO: &[&[(f32, f32)]] = &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)]];
    const ONE: &[&[(f32, f32)]] = &[&[(0.4, 0.8), (1.2, 0.0), (1.2, 4.0)]];
    const TWO: &[&[(f32, f32)]] = &[&[
        (0.0, 0.0),
        (2.0, 0.0),
        (2.0, 2.0),
        (0.0, 2.0),
        (0.0, 4.0),
        (2.0, 4.0),
    ]];
    const THREE: &[&[(f32, f32)]] = &[
        &[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0)],
        &[(0.4, 2.0), (2.0, 2.0)],
    ];
    const FOUR: &[&[(f32, f32)]] = &[
        &[(0.0, 0.0), (0.0, 2.4), (2.0, 2.4)],
        &[(1.6, 0.8), (1.6, 4.0)],
    ];
    const FIVE: &[&[(f32, f32)]] = &[&[
        (2.0, 0.0),
        (0.0, 0.0),
        (0.0, 2.0),
        (2.0, 2.0),
        (2.0, 4.0),
        (0.0, 4.0),
    ]];
    const SIX: &[&[(f32, f32)]] = &[&[
        (2.0, 0.0),
        (0.0, 0.0),
        (0.0, 4.0),
        (2.0, 4.0),
        (2.0, 2.0),
        (0.0, 2.0),
    ]];
    const SEVEN: &[&[(f32, f32)]] = &[&[(0.0, 0.0), (2.0, 0.0), (0.8, 4.0)]];
    const EIGHT: &[&[(f32, f32)]] = &[
        &[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)],
        &[(0.0, 2.0), (2.0, 2.0)],
    ];
    const NINE: &[&[(f32, f32)]] = &[&[
        (0.0, 4.0),
        (2.0, 4.0),
        (2.0, 0.0),
        (0.0, 0.0),
        (0.0, 2.0),
        (2.0, 2.0),
    ]];
    const DOT: &[&[(f32, f32)]] = &[&[(0.2, 3.6), (0.2, 4.0)]];
    const TIMES: &[&[(f32, f32)]] = &[&[(0.6, 1.4), (2.0, 2.8)], &[(2.0, 1.4), (0.6, 2.8)]];
    match c {
        '0' => (ZERO, 2.0),
        '1' => (ONE, 2.0),
        '2' => (TWO, 2.0),
        '3' => (THREE, 2.0),
        '4' => (FOUR, 2.0),
        '5' => (FIVE, 2.0),
        '6' => (SIX, 2.0),
        '7' => (SEVEN, 2.0),
        '8' => (EIGHT, 2.0),
        '9' => (NINE, 2.0),
        '.' => (DOT, 0.4),
        '×' => (TIMES, 2.6),
        _ => (&[], 0.0),
    }
}

/// Gap between label characters, in glyph units.
const GAP: f32 = 1.0;

/// Width of a label's characters, in glyph units.
fn advance(text: &str) -> f32 {
    let widths: f32 = text.chars().map(|c| glyph(c).1).sum();
    widths + GAP * text.chars().count().saturating_sub(1) as f32
}

/// Size of the label showing `text`, points.
fn label_size(text: &str) -> (f32, f32) {
    (
        advance(text) * UNIT + 2.0 * LABEL_PAD,
        4.0 * UNIT + 2.0 * LABEL_PAD,
    )
}

/// Draws the label `text` with its top-left corner at `(x, y)`.
fn label(x: f32, y: f32, text: &str, ops: &mut Vec<Op>) {
    let mut strokes = Path::default();
    let mut pen = 0.0f32;
    for (k, c) in text.chars().enumerate() {
        if k > 0 {
            pen += GAP;
        }
        let (lines, w) = glyph(c);
        for line in lines {
            for (j, &(gx, gy)) in line.iter().enumerate() {
                let (px, py) = (x + LABEL_PAD + (pen + gx) * UNIT, y + LABEL_PAD + gy * UNIT);
                strokes.segs.push(if j == 0 {
                    PathSeg::MoveTo(px, py)
                } else {
                    PathSeg::LineTo(px, py)
                });
            }
        }
        pen += w;
    }
    let (w, h) = label_size(text);
    let bg = Rect::new(x, y, w, h);
    with_alpha(LABEL_BG[3], ops, |ops| {
        ops.push(Op::FillRect {
            rect: bg,
            color: LABEL_BG,
        });
    });
    ops.push(Op::StrokePath {
        path: strokes,
        color: LABEL_FG,
        width: STROKE,
        dash: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_print_to_two_decimals_at_most() {
        assert_eq!(size_text([120.0, 48.5]), "120×48.5");
        assert_eq!(size_text([595.28, 841.89]), "595.28×841.89");
        assert_eq!(size_text([0.004, 10.0]), "0×10");
        assert_eq!(size_text([-0.0, 3.333]), "0×3.33");
    }

    #[test]
    fn every_label_character_has_strokes() {
        for c in "0123456789.×".chars() {
            let (lines, w) = glyph(c);
            assert!(!lines.is_empty() && w > 0.0, "{c}");
        }
    }

    #[test]
    fn rings_only_for_the_edges_an_element_has() {
        let mut ops = Vec::new();
        draw(
            &[DebugBox {
                rect: Rect::new(10.0, 10.0, 50.0, 20.0),
                border: [0.0; 4],
                padding: [2.0; 4],
                margin: [0.0; 4],
                size: [50.0, 20.0],
                shown: (10.0, 30.0),
                ctm: None,
            }],
            &mut ops,
            &mut Vec::new(),
        );
        let fills: Vec<Color> = ops
            .iter()
            .filter_map(|op| match op {
                Op::FillPath { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        // Padding only; the content box is stroked.
        assert_eq!(fills, [PADDING]);
        let content = ops.iter().find_map(|op| match op {
            Op::StrokePath { path, color, .. } if *color == CONTENT => Some(path),
            _ => None,
        });
        assert_eq!(
            content.unwrap().segs[..3],
            [
                PathSeg::MoveTo(12.0, 12.0),
                PathSeg::LineTo(58.0, 12.0),
                PathSeg::LineTo(58.0, 28.0)
            ]
        );
        // No text: labels are drawn with lines.
        assert!(!ops.iter().any(|op| matches!(op, Op::Text(_))));
    }
}
