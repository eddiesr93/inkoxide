//! Float containers: the root, a column box or a cell with floated
//! children, laid out by this module instead of Taffy, as tables are.
//!
//! In-flow children stack top to bottom like the items of a column
//! flexbox (margins without collapsing, `rowGap`, `alignItems` and
//! `alignSelf`, auto margins), each laid out by Taffy at the width this
//! module gives it. Floats are placed with a [`FloatContext`] where they
//! occur: shrink-to-fit, at their side. Text after a float wraps beside
//! it, through line boxes; other boxes after it are narrowed to the room
//! beside it, or moved below it when that is too narrow for them. `clear`
//! moves a box below the floats of its sides. Absolutely positioned
//! children are placed against the padding box.
//!
//! Validation keeps the rest out: no `flexGrow` or `flexBasis` on the
//! children, no `justifyContent` or `alignContent` on the container.

use std::sync::Arc;

use super::float::{FloatContext, LineBoxes, Side};
use super::{AvailableSpace, Geometry, LayoutCtx, NodeIdx, RootMode, Size, Tree};
use crate::schema::{
    AlignItems, AlignSelf, Clear, Display, Edges, Float, Length, LengthPercent, NodeKind, Position,
    Style,
};
use crate::InkoxideError;

/// Tolerance of widths, points.
const EPS: f32 = 1e-3;

/// A float container's children laid out in its content box.
#[derive(Debug, Default)]
pub(crate) struct FlowLayout {
    /// Content size: the width laid out in, and the height of the in-flow
    /// children and the floats.
    pub size: Size<f32>,
    /// Each child and its descendants, in the content box (when
    /// collected).
    pub geos: Vec<(NodeIdx, Geometry)>,
    /// The line boxes of the text that wraps beside floats.
    pub wraps: Vec<(NodeIdx, Arc<LineBoxes>)>,
    /// Per float: the bottom of its group (the floats beside it and the
    /// lines and boxes beside them), in the content box.
    pub groups: Vec<(NodeIdx, f32)>,
}

/// What a text leaf measured beside floats reports: its content height,
/// and the bottom of its last line that starts beside a float.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wrapped {
    pub height: f32,
    pub wrapped: f32,
}

/// A length against `base`: `None` for `auto`.
fn len(l: Length, base: f32) -> Option<f32> {
    match l {
        Length::Points(v) => Some(v),
        Length::Percent(p) => Some(p * base),
        Length::Auto => None,
    }
}

/// A height against the container's height `parent_h`: `None` for `auto`,
/// and for a percentage of a height not known.
fn len_h(l: Option<Length>, parent_h: Option<f32>) -> Option<f32> {
    match l? {
        Length::Points(v) => Some(v),
        Length::Percent(p) => parent_h.map(|h| p * h),
        Length::Auto => None,
    }
}

fn len_pct(l: LengthPercent, base: f32) -> f32 {
    match l {
        LengthPercent::Points(v) => v,
        LengthPercent::Percent(p) => p * base,
    }
}

/// Margins against the container width `w`: `None` for `auto`.
fn margins(s: &Style, w: f32) -> [Option<f32>; 4] {
    let Some(m) = s.margin else {
        return [Some(0.0); 4];
    };
    [m.top, m.right, m.bottom, m.left].map(|v| v.map_or(Some(0.0), |l| len(l, w)))
}

/// Padding and border of a box, against the container width `w`.
fn insets(s: &Style, w: f32) -> (Edges<f32>, Edges<f32>) {
    let p = s.padding.unwrap_or_default();
    let pad = |v: Option<LengthPercent>| v.map_or(0.0, |l| len_pct(l, w).max(0.0));
    let b = s.border_width.unwrap_or_default();
    let bor = |v: Option<f32>| v.unwrap_or(0.0).max(0.0);
    (
        Edges {
            top: pad(p.top),
            right: pad(p.right),
            bottom: pad(p.bottom),
            left: pad(p.left),
        },
        Edges {
            top: bor(b.top),
            right: bor(b.right),
            bottom: bor(b.bottom),
            left: bor(b.left),
        },
    )
}

/// How a child sits across the container.
#[derive(Clone, Copy, PartialEq)]
enum Align {
    Start,
    End,
    Center,
    Stretch,
}

fn align(container: &Style, s: &Style) -> Align {
    let from_items = |a: Option<AlignItems>| match a {
        None | Some(AlignItems::Stretch) => Align::Stretch,
        Some(AlignItems::FlexStart | AlignItems::Start | AlignItems::Baseline) => Align::Start,
        Some(AlignItems::FlexEnd | AlignItems::End) => Align::End,
        Some(AlignItems::Center) => Align::Center,
    };
    match s.align_self {
        None | Some(AlignSelf::Auto) => from_items(container.align_items),
        Some(AlignSelf::Stretch) => Align::Stretch,
        Some(AlignSelf::FlexStart | AlignSelf::Start | AlignSelf::Baseline) => Align::Start,
        Some(AlignSelf::FlexEnd | AlignSelf::End) => Align::End,
        Some(AlignSelf::Center) => Align::Center,
    }
}

/// `v` within `min..=max`, the minimum winning (CSS).
fn clamp(v: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let v = max.map_or(v, |m| v.min(m));
    min.map_or(v, |m| v.max(m))
}

/// Whether node `idx` of `tree` holds floats: the root, a box or a cell
/// with a displayed, floated child.
pub(super) fn holds_floats(tree: &Tree<'_>, idx: usize) -> bool {
    tree.nodes[idx].holds_children()
        && tree.children(idx as NodeIdx).any(|c| {
            tree.nodes[c as usize].node().is_some_and(|n| {
                n.style.floats().is_some() && n.style.display != Some(Display::None)
            })
        })
}

/// A child being laid out: the container's width and the height its
/// children's percentages resolve against.
struct Item<'t, 'a> {
    tree: &'t Tree<'a>,
    width: f32,
    parent_h: Option<f32>,
}

impl Item<'_, '_> {
    /// Border-box width and height of child `c`, `width` wide (`None`:
    /// its intrinsic `mode` width), as Taffy lays it out.
    fn size(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        width: AvailableSpace,
    ) -> Result<Size<f32>, InkoxideError> {
        self.sized(ctx, c, width, false)
    }

    /// [`size`](Self::size), with `fit`: a definite `width` is only room.
    fn sized(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        width: AvailableSpace,
        fit: bool,
    ) -> Result<Size<f32>, InkoxideError> {
        let mode = RootMode::Item {
            width,
            fit,
            height: None,
            parent_h: self.parent_h,
        };
        Ok(ctx.layout_subtree(self.tree, c, mode, false)?.size)
    }

    /// The width of child `c` (style `s`) with `avail` of room for its
    /// margin box: its own width, stretched, or shrink-to-fit.
    fn width(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        s: &Style,
        avail: f32,
        stretch: bool,
    ) -> Result<f32, InkoxideError> {
        // Never narrower than its padding and border.
        let (pad, bor) = insets(s, self.width);
        let edges = pad.left + pad.right + bor.left + bor.right;
        Ok(self.width_unclamped(ctx, c, s, avail, stretch)?.max(edges))
    }

    fn width_unclamped(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        s: &Style,
        avail: f32,
        stretch: bool,
    ) -> Result<f32, InkoxideError> {
        let w = self.width;
        let min = s.min_width.and_then(|l| len(l, w));
        let max = s.max_width.and_then(|l| len(l, w));
        if let Some(own) = s.width.and_then(|l| len(l, w)) {
            return Ok(clamp(own, min, max));
        }
        let [_, mr, _, ml] = margins(s, w);
        let room = avail - ml.unwrap_or(0.0) - mr.unwrap_or(0.0);
        if stretch && ml.is_some() && mr.is_some() {
            return Ok(clamp(room, min, max));
        }
        // A leaf takes what it measures in the room (text the width of its
        // widest line); a box fits its content to it.
        let leaf = self.tree.nodes[c as usize]
            .node()
            .is_some_and(|n| !matches!(n.kind, NodeKind::Box { .. }));
        if leaf {
            let w = self.fit_leaf(ctx, c, s, avail)?.width;
            return Ok(clamp(w, min, max));
        }
        let lo = self.size(ctx, c, AvailableSpace::MinContent)?.width;
        let hi = self.size(ctx, c, AvailableSpace::MaxContent)?.width;
        Ok(clamp(hi.min(lo.max(room)), min, max))
    }

    /// Leaf `c` (style `s`) measured in `room` for its margin box, within
    /// its own minimum and maximum width, as Taffy measures a leaf that
    /// does not stretch: text as wide as its widest line there.
    fn fit_leaf(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        s: &Style,
        room: f32,
    ) -> Result<Size<f32>, InkoxideError> {
        let w = self.width;
        let min = s.min_width.and_then(|l| len(l, w));
        let max = s.max_width.and_then(|l| len(l, w));
        let [_, mr, _, ml] = margins(s, w);
        let room = room - ml.unwrap_or(0.0) - mr.unwrap_or(0.0);
        let available = clamp(room, min, max).max(0.0);
        self.sized(ctx, c, AvailableSpace::Definite(available), true)
    }

    /// The border box of child `c` with `avail` of room for its margin
    /// box: [`width`](Self::width), and the height there.
    fn item_size(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        s: &Style,
        avail: f32,
        stretch: bool,
    ) -> Result<Size<f32>, InkoxideError> {
        let w = self.width(ctx, c, s, avail, stretch)?;
        let leaf = self.tree.nodes[c as usize]
            .node()
            .is_some_and(|n| !matches!(n.kind, NodeKind::Box { .. }));
        let [_, mr, _, ml] = margins(s, self.width);
        let fits = !(stretch && ml.is_some() && mr.is_some());
        if leaf && fits && s.width.is_none() {
            // Measured where it was fitted, as Taffy keeps it.
            let h = self.fit_leaf(ctx, c, s, avail)?.height;
            return Ok(Size {
                width: w,
                height: h,
            });
        }
        let h = self.size(ctx, c, AvailableSpace::Definite(w))?.height;
        Ok(Size {
            width: w,
            height: h,
        })
    }

    /// The narrowest margin box of child `c`: its own width, else its
    /// min-content width, with its margins.
    fn min_width(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        s: &Style,
    ) -> Result<f32, InkoxideError> {
        let w = self.width;
        let [_, mr, _, ml] = margins(s, w);
        let m = ml.unwrap_or(0.0) + mr.unwrap_or(0.0);
        let min = s.min_width.and_then(|l| len(l, w));
        let max = s.max_width.and_then(|l| len(l, w));
        let own = match s.width.and_then(|l| len(l, w)) {
            Some(own) => own,
            None => self.size(ctx, c, AvailableSpace::MinContent)?.width,
        };
        Ok(clamp(own, min, max) + m)
    }
}

/// Lays out the children of float container `idx` in a content box
/// `width` wide; `height` is its final content height, when known (the
/// final layout: auto margins share the free room), and `parent_h` the
/// height percentages of its children resolve against. `collect` also
/// lays out each child's subtree at its final size.
pub(super) fn layout(
    ctx: &mut LayoutCtx<'_>,
    tree: &Tree<'_>,
    idx: NodeIdx,
    width: f32,
    height: Option<f32>,
    parent_h: Option<f32>,
    collect: bool,
) -> Result<FlowLayout, InkoxideError> {
    let container = tree.nodes[idx as usize].style();
    let item = Item {
        tree,
        width,
        parent_h,
    };
    let gap = container
        .row_gap
        .or(container.gap)
        .map_or(0.0, |g| match g {
            LengthPercent::Points(v) => v,
            LengthPercent::Percent(p) => parent_h.map_or(0.0, |h| p * h),
        });
    let mut floats = FloatContext::new(width);
    let mut out = FlowLayout::default();
    // Per child: its border box, margins, and the kind of placement.
    let mut placed: Vec<(NodeIdx, Geometry, Kind)> = Vec::new();
    let mut cursor = 0.0f32;
    let mut any_in_flow = false;
    let mut bottom = 0.0f32;
    let mut float_ids: Vec<(NodeIdx, usize)> = Vec::new();
    for c in tree.children(idx) {
        let Some(node) = tree.nodes[c as usize].node() else {
            continue;
        };
        let s = &node.style;
        if s.display == Some(Display::None) {
            continue;
        }
        if s.position == Some(Position::Absolute) {
            placed.push((c, Geometry::default(), Kind::Absolute));
            continue;
        }
        let [mt, mr, mb, ml] = margins(s, width);
        let cleared = match s.clear {
            Some(Clear::Left) => floats.clear_y(true, false),
            Some(Clear::Right) => floats.clear_y(false, true),
            Some(Clear::Both) => floats.clear_y(true, true),
            Some(Clear::None) | None => None,
        };
        if let Some(side) = s.floats() {
            let side = if side == Float::Left {
                Side::Left
            } else {
                Side::Right
            };
            let (mt, mr, mb, ml) = (
                mt.unwrap_or(0.0),
                mr.unwrap_or(0.0),
                mb.unwrap_or(0.0),
                ml.unwrap_or(0.0),
            );
            let Size {
                width: w,
                height: h,
            } = item.item_size(ctx, c, s, width, false)?;
            let min_y = cleared.map_or(cursor, |y| y.max(cursor));
            let f = floats.place(side, w + ml + mr, h + mt + mb, min_y);
            let geo = Geometry {
                x: f.x + ml,
                y: f.y + mt,
                w,
                h,
                margin: Edges {
                    top: mt,
                    right: mr,
                    bottom: mb,
                    left: ml,
                },
                ..Geometry::default()
            };
            bottom = bottom.max(f.bottom());
            float_ids.push((c, placed.len()));
            placed.push((c, geo, Kind::Float(f.y, f.bottom())));
            continue;
        }
        if any_in_flow {
            cursor += gap;
        }
        any_in_flow = true;
        let top_margin = mt.unwrap_or(0.0);
        // Its border box goes below the floats it clears.
        let mut top = cursor + top_margin;
        if let Some(y) = cleared {
            top = top.max(y);
        }
        let a = align(container, s);
        let stretch = a == Align::Stretch;
        let beside = floats.next_change(top).is_some();
        let is_text = matches!(node.kind, NodeKind::Text { .. });
        let mut kind = Kind::InFlow;
        let geo =
            if beside && is_text && stretch && s.width.is_none() && ml.is_some() && mr.is_some() {
                // Text wraps beside the floats, line by line.
                let (ml, mr) = (ml.unwrap_or(0.0), mr.unwrap_or(0.0));
                let (pad, bor) = insets(s, width);
                let w = clamp(
                    width - ml - mr,
                    s.min_width.and_then(|l| len(l, width)),
                    s.max_width.and_then(|l| len(l, width)),
                );
                let cx = ml + bor.left + pad.left;
                let cy = top + bor.top + pad.top;
                let cw = (w - bor.left - pad.left - pad.right - bor.right).max(0.0);
                let bands = floats.bands(cx, cy, cw);
                let boxes = Arc::new(LineBoxes {
                    width: cw,
                    bands: bands.into(),
                });
                let measured = ctx.leaves.measure_lines(node, &boxes);
                let edges_h = bor.top + pad.top + pad.bottom + bor.bottom;
                let h = clamp(
                    len_h(s.height, parent_h).unwrap_or(measured.height + edges_h),
                    len_h(s.min_height, parent_h),
                    len_h(s.max_height, parent_h),
                );
                out.wraps.push((c, boxes));
                kind = Kind::Wrapped(cy + measured.wrapped);
                Geometry {
                    x: ml,
                    y: top,
                    w,
                    h,
                    border: bor,
                    padding: pad,
                    margin: Edges {
                        top: top_margin,
                        right: mr,
                        bottom: mb.unwrap_or(0.0),
                        left: ml,
                    },
                }
            } else {
                // A box: narrowed to the room beside the floats, or moved
                // below them when it does not fit there.
                let need = if beside {
                    item.min_width(ctx, c, s)?
                } else {
                    0.0
                };
                loop {
                    let (left, right) = floats.band(top, 0.0);
                    let room = width - left - right;
                    if beside && need > room + EPS {
                        if let Some(y) = floats.next_change(top) {
                            top = y;
                            continue;
                        }
                    }
                    let Size {
                        width: w,
                        height: h,
                    } = item.item_size(ctx, c, s, room, stretch)?;
                    let (l2, r2) = floats.band(top, h);
                    if beside && w + ml.unwrap_or(0.0) + mr.unwrap_or(0.0) > width - l2 - r2 + EPS {
                        // A float lower down narrows it: below that one.
                        if let Some(y) = floats.next_change(top) {
                            top = y;
                            continue;
                        }
                    }
                    let (ml_v, mr_v) = (ml.unwrap_or(0.0), mr.unwrap_or(0.0));
                    let x = match (ml, mr) {
                        (None, None) => left + (room - w) / 2.0,
                        (None, Some(_)) => left + room - mr_v - w,
                        (Some(_), None) => left + ml_v,
                        _ => match a {
                            Align::Start | Align::Stretch => left + ml_v,
                            Align::End => left + room - mr_v - w,
                            Align::Center => left + ml_v + (room - ml_v - mr_v - w) / 2.0,
                        },
                    };
                    // Auto margins resolve to the room they take.
                    let resolved = |m: Option<f32>, auto: f32| m.unwrap_or(auto);
                    break Geometry {
                        x,
                        y: top,
                        w,
                        h,
                        margin: Edges {
                            top: top_margin,
                            right: resolved(mr, left + room - x - w),
                            bottom: mb.unwrap_or(0.0),
                            left: resolved(ml, x - left),
                        },
                        ..Geometry::default()
                    };
                }
            };
        cursor = geo.y + geo.h + mb.unwrap_or(0.0);
        bottom = bottom.max(cursor);
        if mt.is_none() || mb.is_none() {
            kind = match kind {
                Kind::InFlow => Kind::AutoVertical(mt.is_none(), mb.is_none()),
                k => k,
            };
        }
        placed.push((c, geo, kind));
    }

    // Auto vertical margins share the free room of a container whose
    // height is known.
    if let Some(h) = height {
        let autos: usize = placed
            .iter()
            .map(|(_, _, k)| match k {
                Kind::AutoVertical(t, b) => usize::from(*t) + usize::from(*b),
                _ => 0,
            })
            .sum();
        let free = h - bottom;
        if autos > 0 && free > EPS && float_ids.is_empty() {
            let share = free / autos as f32;
            let mut moved = 0.0f32;
            for (_, g, k) in &mut placed {
                match *k {
                    Kind::AutoVertical(t, b) => {
                        if t {
                            moved += share;
                            g.margin.top += share;
                        }
                        g.y += moved;
                        if b {
                            moved += share;
                            g.margin.bottom += share;
                        }
                    }
                    Kind::InFlow | Kind::Wrapped(_) => g.y += moved,
                    _ => {}
                }
            }
            bottom += moved;
        }
    }

    // Relative offsets move a box where it is painted, not its room.
    for (c, g, k) in &mut placed {
        if matches!(k, Kind::Absolute) {
            continue;
        }
        let Some(s) = tree.nodes[*c as usize].node().map(|n| &n.style) else {
            continue;
        };
        if let Some(inset) = s.inset {
            let off = |a: Option<Length>, b: Option<Length>, base: f32| {
                a.and_then(|l| len(l, base))
                    .or_else(|| b.and_then(|l| len(l, base)).map(|v| -v))
                    .unwrap_or(0.0)
            };
            g.x += off(inset.left, inset.right, width);
            g.y += off(inset.top, inset.bottom, parent_h.unwrap_or(0.0));
        }
    }

    // Groups: a float, the floats beside it, and what wraps beside them.
    for &(f, k) in &float_ids {
        let Kind::Float(top, fb) = placed[k].2 else {
            continue;
        };
        let mut end = fb;
        for (_, g, kind) in &placed {
            match *kind {
                Kind::Float(t, b) if t < fb && b > top => end = end.max(b),
                Kind::Wrapped(wrapped) if g.y < fb && g.y + g.h > top => {
                    end = end.max(wrapped.min(g.y + g.h));
                }
                Kind::InFlow | Kind::AutoVertical(..) if g.y < fb && g.y + g.h > top => {
                    end = end.max(g.y + g.h);
                }
                _ => {}
            }
        }
        out.groups.push((f, end));
    }

    let content_h = bottom;
    out.size = Size {
        width,
        height: content_h,
    };
    if !collect {
        return Ok(out);
    }
    let box_h = height.unwrap_or(content_h);
    let (pad, _) = insets(container, width);
    for (c, mut g, kind) in placed {
        if matches!(kind, Kind::Absolute) {
            let s = tree.nodes[c as usize].style();
            g = absolute(ctx, &item, c, s, pad, box_h)?;
        }
        if matches!(kind, Kind::Wrapped(_)) {
            // A text leaf: no subtree.
            out.geos.push((c, g));
            continue;
        }
        let mode = RootMode::Item {
            width: AvailableSpace::Definite(g.w),
            fit: false,
            height: Some(g.h),
            parent_h,
        };
        let sub = ctx.layout_subtree(tree, c, mode, true)?;
        for (n, ng) in sub.geos {
            if n == c {
                out.geos.push((
                    c,
                    Geometry {
                        border: ng.border,
                        padding: ng.padding,
                        ..g
                    },
                ));
            } else {
                out.geos.push((n, ng.offset(g.x, g.y)));
            }
        }
    }
    Ok(out)
}

/// How a child was placed.
#[derive(Debug, Clone, Copy)]
enum Kind {
    InFlow,
    /// In flow, with an auto top or bottom margin.
    AutoVertical(bool, bool),
    /// Text wrapped beside floats, with the bottom of its last line that
    /// starts beside one.
    Wrapped(f32),
    /// A float, with the top and bottom of its margin box.
    Float(f32, f32),
    Absolute,
}

/// The border box of absolutely positioned child `c`, against the
/// container's padding box (`pad` around a content box `item.width` ×
/// `height`).
fn absolute(
    ctx: &mut LayoutCtx<'_>,
    item: &Item<'_, '_>,
    c: NodeIdx,
    s: &Style,
    pad: Edges<f32>,
    height: f32,
) -> Result<Geometry, InkoxideError> {
    let (pw, ph) = (
        item.width + pad.left + pad.right,
        height + pad.top + pad.bottom,
    );
    let x_of = |l: Option<Length>| l.and_then(|l| len(l, pw));
    let y_of = |l: Option<Length>| l.and_then(|l| len(l, ph));
    let (left, right, top, bottom) = match s.inset {
        Some(i) => (x_of(i.left), x_of(i.right), y_of(i.top), y_of(i.bottom)),
        None => (None, None, None, None),
    };
    let [mt, mr, mb, ml] = margins(s, pw).map(|m| m.unwrap_or(0.0));
    let min_w = s.min_width.and_then(|l| len(l, pw));
    let max_w = s.max_width.and_then(|l| len(l, pw));
    let w = match (s.width.and_then(|l| len(l, pw)), left, right) {
        (Some(w), ..) => clamp(w, min_w, max_w),
        (None, Some(l), Some(r)) => clamp(pw - l - r - ml - mr, min_w, max_w),
        _ => {
            let lo = item.size(ctx, c, AvailableSpace::MinContent)?.width;
            let hi = item.size(ctx, c, AvailableSpace::MaxContent)?.width;
            let room = pw - left.unwrap_or(0.0) - right.unwrap_or(0.0) - ml - mr;
            clamp(hi.min(lo.max(room)), min_w, max_w)
        }
    };
    let mode = RootMode::Item {
        width: AvailableSpace::Definite(w),
        fit: false,
        height: None,
        parent_h: Some(ph),
    };
    let mut h = ctx.layout_subtree(item.tree, c, mode, false)?.size.height;
    if s.height.is_none() {
        if let (Some(t), Some(b)) = (top, bottom) {
            h = (ph - t - b - mt - mb).max(0.0);
        }
    }
    let x = match (left, right) {
        (Some(l), _) => l + ml,
        (None, Some(r)) => pw - r - mr - w,
        (None, None) => pad.left + ml,
    } - pad.left;
    let y = match (top, bottom) {
        (Some(t), _) => t + mt,
        (None, Some(b)) => ph - b - mb - h,
        (None, None) => pad.top + mt,
    } - pad.top;
    Ok(Geometry {
        x,
        y,
        w,
        h,
        margin: Edges {
            top: mt,
            right: mr,
            bottom: mb,
            left: ml,
        },
        ..Geometry::default()
    })
}

/// The content height of node `idx` when its style sets it in points: the
/// height its children's percentages resolve against.
pub(super) fn definite_content_height(tree: &Tree<'_>, idx: NodeIdx) -> Option<f32> {
    let s = tree.nodes[idx as usize].style();
    let Some(Length::Points(h)) = s.height else {
        return None;
    };
    let p = s.padding.unwrap_or_default();
    let pad = |v: Option<LengthPercent>| match v {
        Some(LengthPercent::Points(x)) => x.max(0.0),
        _ => 0.0,
    };
    let b = s.border_width.unwrap_or_default();
    let edges = pad(p.top) + pad(p.bottom) + b.top.unwrap_or(0.0) + b.bottom.unwrap_or(0.0);
    Some((h - edges).max(0.0))
}

/// The min-content or max-content width of float container `idx`: its
/// widest child, or for the maximum, its floats side by side when wider
/// (side by side up to each `clear`).
pub(super) fn intrinsic_width(
    ctx: &mut LayoutCtx<'_>,
    tree: &Tree<'_>,
    idx: NodeIdx,
    mode: AvailableSpace,
    parent_h: Option<f32>,
) -> Result<f32, InkoxideError> {
    let item = Item {
        tree,
        width: 0.0,
        parent_h,
    };
    let (mut widest, mut row, mut rows) = (0.0f32, 0.0f32, 0.0f32);
    for c in tree.children(idx) {
        let Some(node) = tree.nodes[c as usize].node() else {
            continue;
        };
        let s = &node.style;
        if s.display == Some(Display::None) || s.position == Some(Position::Absolute) {
            continue;
        }
        let [_, mr, _, ml] = margins(s, 0.0).map(|m| m.unwrap_or(0.0));
        let w = item.size(ctx, c, mode)?.width + ml + mr;
        if s.clear.is_some_and(|c| c != Clear::None) {
            row = 0.0;
        }
        if s.floats().is_some() && mode == AvailableSpace::MaxContent {
            row += w;
            rows = rows.max(row);
        } else {
            widest = widest.max(w);
        }
    }
    Ok(widest.max(rows))
}

/// Whether node `idx` is a column box with children, which `flow.rs`
/// lays out as Taffy does: tests check that.
#[cfg(test)]
pub(super) fn column_box(tree: &Tree<'_>, idx: NodeIdx) -> bool {
    let ln = &tree.nodes[idx as usize];
    ln.holds_children()
        && !ln.children.is_empty()
        && ln.style().display != Some(Display::None)
        && ln.style().column()
}
