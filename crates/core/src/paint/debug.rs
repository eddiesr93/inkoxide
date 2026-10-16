//! `debug` outlines: the margin, border, padding and content boxes of an
//! element, and its size as a small label, drawn over the page; for a
//! paragraph, also its lines and the runs on each line.
//!
//! The box painter records a [`DebugBox`] for each element to outline as
//! it paints it (it knows the element's boxes on this page, split ones
//! included); the page painter draws them last, on top of everything.
//! Labels are drawn with lines, not text: they need no font and add
//! nothing to the document's text (copy, search, extraction).

use std::collections::HashMap;

use super::shapes::{push_rounded_rect, NO_RADII};
use super::{with_alpha, Dash, FillRule, Op, Path, PathSeg, Rect};
use crate::schema::{Color, Edges, PageSpec};

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

/// Line box outline (dashed) and baseline of a paragraph line.
pub(crate) const LINE: Color = [0.2, 0.5, 0.9, 0.6];
pub(crate) const BASELINE: Color = [0.85, 0.2, 0.55, 0.8];
const LINE_WIDTH: f32 = 0.35;
/// Runs on a line, alternately purple and teal, so that neighbours differ.
pub(crate) const RUN: [Color; 2] = [[0.6, 0.35, 0.85, 0.2], [0.15, 0.65, 0.7, 0.2]];

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
    pub border: Edges<f32>,
    pub padding: Edges<f32>,
    pub margin: Edges<f32>,
    /// Laid-out border-box size, for the label.
    pub size: [f32; 2],
    /// Top and bottom of the part of the border box shown on this page,
    /// where its label goes.
    pub shown: (f32, f32),
    /// Transform in effect for the element (rotation, `transform`).
    pub ctm: Option<[f32; 6]>,
    /// A paragraph's lines painted on this page.
    pub lines: Vec<DebugLine>,
}

/// One line of a paragraph, outlined: its line box, its baseline and its
/// runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DebugLine {
    /// From the start of the line's first glyph to the end of its last, as
    /// high as the line box.
    pub rect: Rect,
    pub baseline: f32,
    /// Each run's part of the line, in visual order, with the run's index:
    /// the advance of its glyphs, from its font's ascender to its
    /// descender around its own baseline.
    pub runs: Vec<(Rect, u16)>,
}

/// What to outline while painting one part of a page.
#[derive(Debug, Default)]
pub(crate) struct DebugBoxes {
    /// Outline every element (`debug` on the page).
    pub all: bool,
    pub boxes: Vec<DebugBox>,
    /// Boxes of the fixed nodes of a paged flow, which are not sliced like
    /// the flow.
    pub fixed: Vec<DebugBox>,
}

impl DebugBoxes {
    pub fn new(all: bool) -> Self {
        Self {
            all,
            boxes: Vec::new(),
            fixed: Vec::new(),
        }
    }
}

/// Side of a [`Labels`] grid cell, points: about a label's width, so a
/// label touches a few cells.
const CELL: f32 = 32.0;
/// Labels a cell holds. A cell given more counts as taken everywhere:
/// only labels piled on each other (dozens of elements within a few
/// points) get there, and it bounds the work per label.
const CELL_CAP: usize = 64;
/// Cells a label may touch: a guard, as a label is at most a few hundred
/// points wide (rounding far from the page at most doubles that), and
/// coordinates out of `i32` cells land in the last cell. A label over more
/// stays at its first corner and blocks no other.
const MAX_CELLS: i64 = 64;

/// The labels already drawn on a page, in a uniform grid of [`CELL`]
/// points: whether a corner is free is answered from the labels in the
/// cells it touches, not from every label on the page.
#[derive(Debug, Default)]
pub(crate) struct Labels {
    cells: HashMap<(i32, i32), Vec<Rect>>,
}

impl Labels {
    /// The cells `r` touches, or `None` when there are more than
    /// [`MAX_CELLS`]. Overlapping rectangles share a cell: the cell of a
    /// coordinate is monotonic in it, and two overlapping rectangles
    /// share the point at the larger of their left and top edges.
    fn cells(r: Rect) -> Option<impl Iterator<Item = (i32, i32)>> {
        let cell = |v: f32| (v / CELL).floor() as i32;
        let (x0, x1) = (cell(r.x), cell(r.x + r.w));
        let (y0, y1) = (cell(r.y), cell(r.y + r.h));
        let n = (i64::from(x1) - i64::from(x0) + 1) * (i64::from(y1) - i64::from(y0) + 1);
        (n <= MAX_CELLS).then(|| (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y))))
    }

    /// Whether `r` covers no label.
    fn free(&self, r: Rect) -> bool {
        let Some(mut cells) = Self::cells(r) else {
            return false;
        };
        cells.all(|c| {
            self.cells.get(&c).is_none_or(|labels| {
                labels.len() <= CELL_CAP && !labels.iter().any(|l| overlap(*l, r))
            })
        })
    }

    fn insert(&mut self, r: Rect) {
        let Some(cells) = Self::cells(r) else {
            return;
        };
        for c in cells {
            let labels = self.cells.entry(c).or_default();
            // One past the cap marks the cell full.
            if labels.len() <= CELL_CAP {
                labels.push(r);
            }
        }
    }

    /// Where a label of `size` goes, and records it: the first of
    /// `corners` (top-left positions) where it covers no other label, or
    /// the first corner.
    fn place(&mut self, corners: [(f32, f32); 4], (w, h): (f32, f32)) -> (f32, f32) {
        let at = |(x, y): (f32, f32)| Rect::new(x, y, w, h);
        let c = corners
            .into_iter()
            .find(|&c| self.free(at(c)))
            .unwrap_or(corners[0]);
        self.insert(at(c));
        c
    }
}

/// Draws the outlines of `boxes`. `labels` are the labels already on the
/// page: a label moves to another corner of its box rather than cover one.
pub(crate) fn draw(boxes: &[DebugBox], ops: &mut Vec<Op>, labels: &mut Labels) {
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
pub(crate) fn page_box(spec: &PageSpec) -> DebugBox {
    let (m, s) = (spec.margin, spec.size);
    let (w, h) = spec.content_size();
    DebugBox {
        rect: Rect::new(m.left, m.top, w, h),
        border: Edges::default(),
        padding: Edges::default(),
        margin: m,
        size: [s.width, s.height],
        shown: (0.0, s.height),
        ctm: None,
        lines: Vec::new(),
    }
}

/// Fills the area between `outer` and `inner`.
fn ring(outer: Rect, inner: Rect, color: Color, ops: &mut Vec<Op>) {
    let mut path = Path::default();
    push_rounded_rect(&mut path, outer, &NO_RADII);
    push_rounded_rect(&mut path, inner, &NO_RADII);
    with_alpha(ops, 1.0, color[3], |ops| {
        ops.push(Op::FillPath {
            path,
            color,
            rule: FillRule::EvenOdd,
        });
    });
}

fn draw_box(b: &DebugBox, ops: &mut Vec<Op>, labels: &mut Labels) {
    let border = b.rect;
    let padding = border.inset(b.border);
    let content = padding.inset(b.padding);
    let any = |e: Edges<f32>| e.to_array().iter().any(|v| *v != 0.0);
    if any(b.margin) {
        ring(border.outset(b.margin), border, MARGIN, ops);
    }
    if any(b.border) {
        ring(border, padding, BORDER, ops);
    }
    if any(b.padding) {
        ring(padding, content, PADDING, ops);
    }
    if !b.lines.is_empty() {
        lines(&b.lines, ops);
    }
    let mut path = Path::default();
    push_rounded_rect(&mut path, content, &NO_RADII);
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
    let place = if b.ctm.is_some() {
        // Rotated or transformed: its own corner, wherever that lands.
        corners[0]
    } else {
        labels.place(corners, (lw, lh))
    };
    label(place.0, place.1, &text, ops);
}

/// Draws a paragraph's lines: its runs filled, each colour as one path,
/// then the line boxes dashed and the baselines.
fn lines(lines: &[DebugLine], ops: &mut Vec<Op>) {
    for (k, color) in RUN.into_iter().enumerate() {
        let mut path = Path::default();
        let runs = lines.iter().flat_map(|l| &l.runs);
        for (rect, _) in runs.filter(|(_, run)| usize::from(*run) % 2 == k) {
            push_rounded_rect(&mut path, *rect, &NO_RADII);
        }
        if !path.segs.is_empty() {
            with_alpha(ops, 1.0, color[3], |ops| {
                ops.push(Op::FillPath {
                    path,
                    color,
                    rule: FillRule::NonZero,
                });
            });
        }
    }
    let mut boxes = Path::default();
    let mut baselines = Path::default();
    for l in lines {
        push_rounded_rect(&mut boxes, l.rect, &NO_RADII);
        baselines.segs.push(PathSeg::MoveTo(l.rect.x, l.baseline));
        baselines
            .segs
            .push(PathSeg::LineTo(l.rect.x + l.rect.w, l.baseline));
    }
    for (path, color, dash) in [
        (
            boxes,
            LINE,
            Some(Dash {
                pattern: [1.5, 1.0],
                round_cap: false,
            }),
        ),
        (baselines, BASELINE, None),
    ] {
        with_alpha(ops, 1.0, color[3], |ops| {
            ops.push(Op::StrokePath {
                path,
                color,
                width: LINE_WIDTH,
                dash,
            });
        });
    }
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
    with_alpha(ops, 1.0, LABEL_BG[3], |ops| {
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
                border: Edges::default(),
                padding: Edges::all(2.0),
                margin: Edges::default(),
                size: [50.0, 20.0],
                shown: (10.0, 30.0),
                ctm: None,
                lines: Vec::new(),
            }],
            &mut ops,
            &mut Labels::default(),
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

    /// xorshift64: deterministic pseudo-random numbers in `[0, 1)`.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    /// The corners of a label of `size` on the border box `r`, as
    /// `draw_box` tries them.
    fn corners(r: Rect, (lw, lh): (f32, f32)) -> [(f32, f32); 4] {
        let (left, right) = (r.x, r.x + r.w - lw);
        let (top, bottom) = (r.y, r.y + r.h);
        [
            (left, top),
            (right, top),
            (left, bottom - lh),
            (right, bottom - lh),
        ]
    }

    #[test]
    fn the_grid_places_labels_like_a_scan_of_every_label() {
        // The placement before the grid: check every label on the page.
        fn scan(
            labels: &mut Vec<Rect>,
            corners: [(f32, f32); 4],
            (w, h): (f32, f32),
        ) -> (f32, f32) {
            let at = |(x, y): (f32, f32)| Rect::new(x, y, w, h);
            let free = |r: Rect| !labels.iter().any(|l| overlap(*l, r));
            let c = corners
                .into_iter()
                .find(|&c| free(at(c)))
                .unwrap_or(corners[0]);
            labels.push(at(c));
            c
        }
        for seed in 1..=8u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let (mut grid, mut all) = (Labels::default(), Vec::new());
            let mut moved = 0;
            for _ in 0..1500 {
                // Boxes on and around a page, snapped to half points so
                // that edges land on cell boundaries too.
                let snap = |v: f32| (v * 2.0).round() / 2.0;
                let r = Rect::new(
                    snap(rng.next() * 700.0 - 50.0),
                    snap(rng.next() * 900.0 - 50.0),
                    snap(rng.next() * rng.next() * 200.0),
                    snap(rng.next() * rng.next() * 120.0),
                );
                let size = label_size(&size_text([r.w, r.h]));
                let cs = corners(r, size);
                let c = scan(&mut all, cs, size);
                assert_eq!(grid.place(cs, size), c, "seed {seed}, box {r:?}");
                moved += usize::from(c != cs[0]);
            }
            // Labels did move away from each other.
            assert!(moved > 100, "seed {seed}: {moved}");
        }
    }

    /// Placing a label does not look at every label already placed: labels
    /// spread over a large page, and as many piled on a few points, place
    /// in linear time.
    #[test]
    fn placing_many_labels_takes_linear_time() {
        let size = label_size(&size_text([3.0, 3.0]));
        crate::test_timing::assert_linear("labels", 25_000, |n| {
            let start = std::time::Instant::now();
            let mut labels = Labels::default();
            for k in 0..n {
                let r = Rect::new((k % 316) as f32 * 45.0, (k / 316) as f32 * 45.0, 3.0, 3.0);
                labels.place(corners(r, size), size);
            }
            for k in 0..n {
                let r = Rect::new(100.0 + (k % 7) as f32 * 0.01, 100.0, 3.0, 3.0);
                labels.place(corners(r, size), size);
            }
            let took = start.elapsed();
            assert!(
                labels.cells.values().all(|c| c.len() <= CELL_CAP + 1),
                "cells stay bounded"
            );
            took
        });
    }

    #[test]
    fn labels_at_extreme_coordinates_are_placed() {
        let mut labels = Labels::default();
        let size = (20.0, 6.8);
        for x in [1.0e12, -1.0e30, f32::INFINITY, f32::NEG_INFINITY] {
            let far = Rect::new(x, 0.0, 3.0, 3.0);
            for _ in 0..3 {
                assert_eq!(labels.place(corners(far, size), size), (x, 0.0));
            }
        }
        let nan = Rect::new(f32::NAN, 0.0, 3.0, 3.0);
        let c = labels.place(corners(nan, size), size);
        assert!(c.0.is_nan() && c.1 == 0.0);
        // None of them blocks a label on the page.
        let near = Rect::new(0.0, 0.0, 30.0, 30.0);
        assert_eq!(labels.place(corners(near, size), size), (0.0, 0.0));
        // A label over more cells than the guard allows blocks nothing.
        let wide = Rect::new(0.0, 100.0, 4000.0, 10.0);
        assert!(Labels::cells(wide).is_none());
        labels.insert(wide);
        assert!(labels.free(Rect::new(10.0, 100.0, 20.0, 6.8)));
    }
}
