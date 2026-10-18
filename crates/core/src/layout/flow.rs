//! Float containers: the root, a column box or a cell with floated
//! children, laid out by this module instead of Taffy, as tables are.
//!
//! In-flow children stack top to bottom like the items of a column
//! flexbox (margins without collapsing, `rowGap`, `alignItems` and
//! `alignSelf`, auto margins), each laid out by Taffy at the width this
//! module gives it. Floats are placed with a [`FloatContext`] where they
//! occur: shrink-to-fit, at their side. Text keeps its own box, as CSS
//! lays out a paragraph beside floats, and its lines wrap beside them
//! through line boxes. A column box that holds no floats and does not
//! clip is laid out the same way, in the room the floats leave, so that
//! the text inside it wraps beside them line by line (as CSS lays out a
//! block box that is not a formatting context); other boxes are narrowed
//! to the room beside the floats over their whole height, or moved below
//! them when that is too narrow for them. `clear` moves a box below the
//! floats of its sides.
//! Absolutely positioned children are placed against the padding box.
//! When the container's height is known, its free room goes, as in a
//! flexbox, to `auto` top and bottom margins, else to the children that
//! grow (`flexGrow`), else around them (`justifyContent`): the children
//! are placed again with it, the floats with them.
//!
//! Validation keeps the rest out: no `flexBasis` on the children, no
//! `flexGrow` on a float, no `alignContent` on the container.

use std::sync::Arc;

use super::float::{FloatContext, Insets, LineBoxes, Placed, Side};
use super::{AvailableSpace, Geometry, LayoutCtx, NodeIdx, RootMode, Size, Tree};
use crate::schema::{
    AlignItems, AlignSelf, Clear, Display, Edges, Float, JustifyContent, Length, LengthPercent,
    NodeKind, Overflow, Position, Style,
};
use crate::InkoxideError;

/// Tolerance of widths, points.
const EPS: f32 = 1e-3;
/// Deepest a box is laid out beside the floats of its container's
/// container: deeper boxes are narrowed instead. Each level may be placed
/// twice (to share out free room), so the work doubles with each.
const MAX_SHARED_DEPTH: u8 = 8;

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
    /// Laid out beside the floats of its container: the bottom of what is
    /// set beside them, in the content box.
    beside: f32,
}

/// What a text leaf measured beside floats reports: its content height,
/// and the bottom of its last line set beside a float.
#[derive(Debug, Clone, Copy, Default)]
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

/// What a child's style sets across its container, resolved against
/// the width `w` its percentages are of.
struct Across {
    /// Left and right margins: `None` for `auto`.
    left: Option<f32>,
    right: Option<f32>,
    width: Option<f32>,
    min: Option<f32>,
    max: Option<f32>,
}

impl Across {
    fn of(s: &Style, w: f32) -> Self {
        let [left, right] = margins(s, w, |m| [m.left, m.right]);
        Self {
            left,
            right,
            width: s.width.and_then(|l| len(l, w)),
            min: s.min_width.and_then(|l| len(l, w)),
            max: s.max_width.and_then(|l| len(l, w)),
        }
    }

    /// Both margins, `auto` as 0.
    fn margins(&self) -> f32 {
        self.left.unwrap_or(0.0) + self.right.unwrap_or(0.0)
    }

    /// Whether neither margin is `auto`, so that the box may stretch.
    fn definite_margins(&self) -> bool {
        self.left.is_some() && self.right.is_some()
    }

    /// `v` within the minimum and maximum width.
    fn clamp(&self, v: f32) -> f32 {
        clamp(v, self.min, self.max)
    }

    /// Where a border box `w` wide aligned by `a` goes in the room
    /// `left..left + room`, and its margins there, auto margins taking
    /// what they leave: `(x, left margin, right margin)`.
    fn place(&self, a: Align, left: f32, room: f32, w: f32) -> (f32, f32, f32) {
        let (ml, mr) = (self.left.unwrap_or(0.0), self.right.unwrap_or(0.0));
        let x = match (self.left, self.right) {
            (None, None) => left + (room - w) / 2.0,
            (None, Some(_)) => left + room - mr - w,
            (Some(_), None) => left + ml,
            _ => match a {
                Align::Start | Align::Stretch => left + ml,
                Align::End => left + room - mr - w,
                Align::Center => left + ml + (room - ml - mr - w) / 2.0,
            },
        };
        (
            x,
            self.left.unwrap_or(x - left),
            self.right.unwrap_or(left + room - x - w),
        )
    }
}

/// Top and bottom margins against the container width `w`: `None` for
/// `auto`.
fn vertical_margins(s: &Style, w: f32) -> (Option<f32>, Option<f32>) {
    let [top, bottom] = margins(s, w, |m| [m.top, m.bottom]);
    (top, bottom)
}

/// The margins `pick` takes from `s`, against the container width `w`:
/// `None` for `auto`, 0 when not set.
fn margins<const N: usize>(
    s: &Style,
    w: f32,
    pick: impl Fn(&crate::schema::OptEdges<Length>) -> [Option<Length>; N],
) -> [Option<f32>; N] {
    match &s.margin {
        Some(m) => pick(m).map(|v| v.map_or(Some(0.0), |l| len(l, w))),
        None => [Some(0.0); N],
    }
}

/// Padding and border of a box, against the container width `w`.
fn insets(s: &Style, w: f32) -> (Edges<f32>, Edges<f32>) {
    let p = s.padding.unwrap_or_default().resolve(Some(w));
    let pad = |v: Option<f32>| v.unwrap_or(0.0);
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

/// A float container, as its children see it: its content width and the
/// height their percentages resolve against; whether the geometry of its
/// children's subtrees is collected, and how many boxes beside floats it
/// is inside.
struct Container<'t, 'a> {
    tree: &'t Tree<'a>,
    width: f32,
    parent_h: Option<f32>,
    collect: bool,
    depth: u8,
}

impl Container<'_, '_> {
    /// Whether child `c` is a leaf (text, an image, a field), not a box.
    fn leaf(&self, c: NodeIdx) -> bool {
        self.tree.nodes[c as usize]
            .node()
            .is_some_and(|n| !matches!(n.kind, NodeKind::Box { .. }))
    }

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

    /// The width of child `c` (style `s`, `a` across) with `avail` of room
    /// for its margin box: its own width, stretched, or shrink-to-fit.
    fn width(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        (s, a): (&Style, &Across),
        avail: f32,
        stretch: bool,
    ) -> Result<f32, InkoxideError> {
        // Never narrower than its padding and border.
        let (pad, bor) = insets(s, self.width);
        let edges = pad.left + pad.right + bor.left + bor.right;
        Ok(self.width_unclamped(ctx, c, a, avail, stretch)?.max(edges))
    }

    fn width_unclamped(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        a: &Across,
        avail: f32,
        stretch: bool,
    ) -> Result<f32, InkoxideError> {
        if let Some(own) = a.width {
            return Ok(a.clamp(own));
        }
        let room = avail - a.margins();
        if stretch && a.definite_margins() {
            return Ok(a.clamp(room));
        }
        // A leaf takes what it measures in the room (text the width of its
        // widest line); a box fits its content to it.
        if self.leaf(c) {
            return Ok(a.clamp(self.fit_leaf(ctx, c, a, avail)?.width));
        }
        Ok(a.clamp(self.shrink_to_fit(ctx, c, room)?))
    }

    /// The shrink-to-fit width of box `c` in `room`: its max-content
    /// width, at most `room` but no less than its min-content width.
    fn shrink_to_fit(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        room: f32,
    ) -> Result<f32, InkoxideError> {
        let lo = self.size(ctx, c, AvailableSpace::MinContent)?.width;
        let hi = self.size(ctx, c, AvailableSpace::MaxContent)?.width;
        Ok(hi.min(lo.max(room)))
    }

    /// Leaf `c` measured in `room` for its margin box, within its own
    /// minimum and maximum width, as Taffy measures a leaf that does not
    /// stretch: text as wide as its widest line there.
    fn fit_leaf(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        a: &Across,
        room: f32,
    ) -> Result<Size<f32>, InkoxideError> {
        let available = a.clamp(room - a.margins()).max(0.0);
        self.sized(ctx, c, AvailableSpace::Definite(available), true)
    }

    /// The border box of child `c` with `avail` of room for its margin
    /// box: [`width`](Self::width), and the height there.
    fn item_size(
        &self,
        ctx: &mut LayoutCtx<'_>,
        c: NodeIdx,
        (s, a): (&Style, &Across),
        avail: f32,
        stretch: bool,
    ) -> Result<Size<f32>, InkoxideError> {
        let w = self.width(ctx, c, (s, a), avail, stretch)?;
        let fits = !(stretch && a.definite_margins());
        let h = if self.leaf(c) && fits && a.width.is_none() {
            // Measured where it was fitted, as Taffy keeps it.
            self.fit_leaf(ctx, c, a, avail)?.height
        } else {
            self.size(ctx, c, AvailableSpace::Definite(w))?.height
        };
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
        a: &Across,
    ) -> Result<f32, InkoxideError> {
        let own = match a.width {
            Some(own) => own,
            None => self.size(ctx, c, AvailableSpace::MinContent)?.width,
        };
        Ok(a.clamp(own) + a.margins())
    }
}

/// The children of a float container placed in its content box.
#[derive(Default)]
struct Arrangement {
    /// Per child: its border box with its margins, and how it was placed.
    placed: Vec<(NodeIdx, Geometry, Kind)>,
    /// The floats, and where each is in `placed`.
    floats: Vec<(NodeIdx, usize)>,
    wraps: Vec<(NodeIdx, Arc<LineBoxes>)>,
    /// The boxes laid out beside floats, by their place in `placed`.
    shared: Vec<(usize, FlowLayout)>,
    /// The bottom of the in-flow children's margin boxes and of the
    /// floats.
    bottom: f32,
    /// How many top and bottom margins of in-flow children are `auto`.
    autos: usize,
    /// The in-flow children, and the sum of their `flexGrow`.
    in_flow: usize,
    grow: f32,
}

/// How the free room of a container whose height is known is shared
/// out: placed again with it, the children take it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Share {
    /// Each `auto` top and bottom margin of an in-flow child.
    auto_margin: f32,
    /// Height an in-flow child gains per unit of its `flexGrow`.
    per_grow: f32,
    /// Room above the first child, and between two in-flow children.
    lead: f32,
    between: f32,
}

impl Share {
    /// The share of `free` room (positive) for placement `p` of a
    /// container justified by `justify`: `auto` margins take it all, else
    /// the children that grow, else `justifyContent` sets it around them.
    fn of(p: &Arrangement, free: f32, justify: Option<JustifyContent>) -> Self {
        if p.autos > 0 {
            return Self {
                auto_margin: free / p.autos as f32,
                ..Self::default()
            };
        }
        if p.grow > 0.0 {
            // Children whose `flexGrow` adds up to less than 1 take only
            // that part of it (CSS Flexbox §9.7).
            return Self {
                per_grow: free / p.grow.max(1.0),
                ..Self::default()
            };
        }
        let n = p.in_flow as f32;
        match justify {
            Some(JustifyContent::FlexEnd | JustifyContent::End) => Self {
                lead: free,
                ..Self::default()
            },
            Some(JustifyContent::Center) => Self {
                lead: free / 2.0,
                ..Self::default()
            },
            Some(JustifyContent::SpaceBetween) if p.in_flow > 1 => Self {
                between: free / (n - 1.0),
                ..Self::default()
            },
            Some(JustifyContent::SpaceAround) if p.in_flow > 0 => Self {
                lead: free / n / 2.0,
                between: free / n,
                ..Self::default()
            },
            Some(JustifyContent::SpaceEvenly) => Self {
                lead: free / (n + 1.0),
                between: free / (n + 1.0),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}

/// Places the children of float container `idx`, with the free room it
/// shares out, beside the floats `outer` of the containers around it.
fn place(
    ctx: &mut LayoutCtx<'_>,
    container: &Container<'_, '_>,
    idx: NodeIdx,
    share: Share,
    outer: &[Placed],
) -> Result<Arrangement, InkoxideError> {
    let tree = container.tree;
    let width = container.width;
    let style = tree.nodes[idx as usize].style();
    let gap = style.row_gap.or(style.gap).map_or(0.0, |g| match g {
        LengthPercent::Points(v) => v,
        LengthPercent::Percent(p) => container.parent_h.map_or(0.0, |h| p * h),
    });
    // Each displayed child, and whether it is text.
    let kids: Vec<(NodeIdx, &Style, bool)> = tree
        .children(idx)
        .filter_map(|c| {
            let n = tree.nodes[c as usize].node()?;
            let text = matches!(n.kind, NodeKind::Text { .. });
            (n.style.display != Some(Display::None)).then_some((c, &*n.style, text))
        })
        .collect();
    // How far the cursor may still move up from each child on: by
    // negative margins and gaps. Floats that end above that are done with.
    let mut rise = vec![0.0f32; kids.len() + 1];
    for (k, &(_, s, _)) in kids.iter().enumerate().rev() {
        let in_flow = s.position != Some(Position::Absolute) && s.floats().is_none();
        let (mt, mb) = vertical_margins(s, width);
        rise[k] = rise[k + 1]
            + if in_flow {
                gap.min(0.0) + mt.unwrap_or(0.0).min(0.0) + mb.unwrap_or(0.0).min(0.0)
            } else {
                0.0
            };
    }
    let mut floats = FloatContext::seeded(width, outer.to_vec());
    let mut out = Arrangement::default();
    let mut cursor = share.lead;
    let mut any_in_flow = false;
    for (k, &(c, s, text)) in kids.iter().enumerate() {
        if s.position == Some(Position::Absolute) {
            out.placed.push((c, Geometry::default(), Kind::Absolute));
            continue;
        }
        floats.retire(cursor + rise[k]);
        let a = Across::of(s, width);
        let (mt, mb) = vertical_margins(s, width);
        let cleared = s.clear.and_then(|c| floats.clear_y(c));
        if let Some(side) = s.floats() {
            let side = if side == Float::Left {
                Side::Left
            } else {
                Side::Right
            };
            let (mt, mb) = (mt.unwrap_or(0.0), mb.unwrap_or(0.0));
            let (ml, mr) = (a.left.unwrap_or(0.0), a.right.unwrap_or(0.0));
            let size = container.item_size(ctx, c, (s, &a), width, false)?;
            let min_y = cleared.map_or(cursor, |y| y.max(cursor));
            let (mw, mh) = (size.width + ml + mr, size.height + mt + mb);
            let shape = s
                .shape_outside
                .as_ref()
                .map(|shape| super::shape::strips(shape, s.shape_margin.unwrap_or(0.0), mw, mh));
            let f = floats.place(side, (mw, mh), min_y, shape);
            let geo = Geometry {
                x: f.x + ml,
                y: f.y + mt,
                w: size.width,
                h: size.height,
                margin: Edges {
                    top: mt,
                    right: mr,
                    bottom: mb,
                    left: ml,
                },
                ..Geometry::default()
            };
            out.bottom = out.bottom.max(f.bottom());
            out.floats.push((c, out.placed.len()));
            out.placed.push((c, geo, Kind::Float(f.y, f.bottom())));
            continue;
        }
        if any_in_flow {
            cursor += gap + share.between;
        }
        any_in_flow = true;
        out.autos += usize::from(mt.is_none()) + usize::from(mb.is_none());
        let grow = s.flex_grow.unwrap_or(0.0);
        out.in_flow += 1;
        out.grow += grow;
        let (mt, mb) = (
            mt.unwrap_or(share.auto_margin),
            mb.unwrap_or(share.auto_margin),
        );
        // Its border box goes below the floats it clears.
        let mut top = cursor + mt;
        if let Some(y) = cleared {
            top = top.max(y);
        }
        let item = Item {
            c,
            style: s,
            across: &a,
            align: align(style, s),
            grow: grow * share.per_grow,
        };
        let beside = floats.next_change(top).is_some();
        let (geo, kind) = if text && beside {
            let (geo, boxes, wrapped) = wrap_text(ctx, container, &floats, &item, top)?;
            out.wraps.push((c, boxes));
            (geo, Kind::Wrapped(wrapped))
        } else if beside && container.depth < MAX_SHARED_DEPTH && shares_floats(tree, c) {
            let (geo, mut fl) = share_floats(ctx, container, &floats, &item, top)?;
            out.wraps.append(&mut fl.wraps);
            let set = geo.y + geo.border.top + geo.padding.top + fl.beside;
            out.shared.push((out.placed.len(), fl));
            (geo, Kind::Wrapped(set))
        } else {
            let geo = narrow(ctx, container, &floats, &item, top)?;
            (geo, Kind::InFlow)
        };
        let geo = Geometry {
            margin: Edges {
                top: mt,
                bottom: mb,
                ..geo.margin
            },
            ..geo
        };
        cursor = geo.y + geo.h + mb;
        out.bottom = out.bottom.max(cursor);
        out.placed.push((c, geo, kind));
    }
    Ok(out)
}

/// An in-flow child being placed.
struct Item<'s> {
    c: NodeIdx,
    style: &'s Style,
    across: &'s Across,
    align: Align,
    /// The height it gains from the container's free room.
    grow: f32,
}

/// Text child `item` with its border box's top at `top`, beside floats:
/// its own box, as wide as it would be without them, and its lines in
/// the room the floats leave in it. Also returns those line boxes and the
/// bottom of its last line beside a float.
fn wrap_text(
    ctx: &mut LayoutCtx<'_>,
    container: &Container<'_, '_>,
    floats: &FloatContext,
    item: &Item<'_>,
    top: f32,
) -> Result<(Geometry, Arc<LineBoxes>, f32), InkoxideError> {
    let (s, a) = (item.style, item.across);
    let width = container.width;
    let w = container.width(ctx, item.c, (s, a), width, item.align == Align::Stretch)?;
    let (x, ml, mr) = a.place(item.align, 0.0, width, w);
    let (pad, bor) = insets(s, width);
    let cx = x + bor.left + pad.left;
    let cy = top + bor.top + pad.top;
    let cw = (w - bor.left - pad.left - pad.right - bor.right).max(0.0);
    let boxes = Arc::new(LineBoxes {
        width: cw,
        bands: floats.bands(cx, cy, cw).into(),
    });
    let node = container.tree.nodes[item.c as usize]
        .node()
        .expect("a text child is a node");
    let measured = ctx.leaves.measure_lines(node, &boxes);
    let edges_h = bor.top + pad.top + pad.bottom + bor.bottom;
    let parent_h = container.parent_h;
    let h = clamp(
        len_h(s.height, parent_h).unwrap_or(measured.height + edges_h) + item.grow,
        len_h(s.min_height, parent_h),
        len_h(s.max_height, parent_h),
    );
    let geo = Geometry {
        x,
        y: top,
        w,
        h,
        border: bor,
        padding: pad,
        margin: Edges {
            right: mr,
            left: ml,
            ..Edges::default()
        },
    };
    Ok((geo, boxes, cy + measured.wrapped))
}

/// Whether box `c`, beside floats, is laid out in the room they leave,
/// as its container is, so that its text wraps beside them line by line:
/// a column box that holds no floats of its own, does not clip and keeps
/// no aspect ratio, with no child that takes a `flexBasis` (CSS: a block
/// box that is not a formatting context).
fn shares_floats(tree: &Tree<'_>, c: NodeIdx) -> bool {
    let Some(n) = tree.nodes[c as usize].node() else {
        return false;
    };
    let s = &n.style;
    matches!(n.kind, NodeKind::Box { .. })
        && column_box(tree, c)
        && !holds_floats(tree, c as usize)
        && s.overflow != Some(Overflow::Hidden)
        && s.aspect_ratio.is_none()
        && tree
            .children(c)
            .all(|k| tree.nodes[k as usize].style().flex_basis.is_none())
}

/// Box `item` with its border box's top at `top`, beside floats: its own
/// box, as wide as it would be without them, and its children laid out
/// in it as a float container's, beside those floats.
fn share_floats(
    ctx: &mut LayoutCtx<'_>,
    container: &Container<'_, '_>,
    floats: &FloatContext,
    item: &Item<'_>,
    top: f32,
) -> Result<(Geometry, FlowLayout), InkoxideError> {
    let (s, a) = (item.style, item.across);
    let tree = container.tree;
    let width = container.width;
    let w = container.width(ctx, item.c, (s, a), width, item.align == Align::Stretch)?;
    let (x, ml, mr) = a.place(item.align, 0.0, width, w);
    let (pad, bor) = insets(s, width);
    let cx = x + bor.left + pad.left;
    let cy = top + bor.top + pad.top;
    let cw = (w - bor.left - pad.left - pad.right - bor.right).max(0.0);
    let edges_h = bor.top + pad.top + pad.bottom + bor.bottom;
    let parent_h = container.parent_h;
    let own = len_h(s.height, parent_h).map(|h| (h - edges_h).max(0.0));
    let inner = Container {
        tree,
        width: cw,
        parent_h: own,
        collect: container.collect,
        depth: container.depth + 1,
    };
    let seeds = floats.seeds(cx, cy);
    let mut fl = layout_in(ctx, &inner, item.c, own, &seeds)?;
    let h = clamp(
        own.map_or(fl.size.height, |o| o) + edges_h + item.grow,
        len_h(s.min_height, parent_h),
        len_h(s.max_height, parent_h),
    );
    // Taller than its own height or its content (`minHeight`,
    // `flexGrow`): its children share out that room.
    let content = h - edges_h;
    if content > own.unwrap_or(fl.size.height) + EPS {
        fl = layout_in(ctx, &inner, item.c, Some(content), &seeds)?;
    }
    let geo = Geometry {
        x,
        y: top,
        w,
        h,
        border: bor,
        padding: pad,
        margin: Edges {
            right: mr,
            left: ml,
            ..Edges::default()
        },
    };
    Ok((geo, fl))
}

/// Child `item` with its border box's top at `top` or lower: narrowed to
/// the room the floats leave beside it over its whole height, or moved
/// below them where that is narrower than its min-content width.
fn narrow(
    ctx: &mut LayoutCtx<'_>,
    container: &Container<'_, '_>,
    floats: &FloatContext,
    item: &Item<'_>,
    mut top: f32,
) -> Result<Geometry, InkoxideError> {
    let (s, a) = (item.style, item.across);
    let width = container.width;
    let beside = floats.next_change(top).is_some();
    let need = if beside {
        container.min_width(ctx, item.c, a)?
    } else {
        0.0
    };
    let mut taken = floats.band(top, 0.0);
    // Each turn moves `top` down to a float bottom, or takes more room
    // from a side, up to a float edge; there are finitely many of both.
    let (taken, size) = loop {
        if beside && need > taken.room(width) + EPS {
            if let Some(y) = floats.next_change(top) {
                top = y;
                taken = floats.band(top, 0.0);
                continue;
            }
        }
        let stretch = item.align == Align::Stretch;
        let mut size = container.item_size(ctx, item.c, (s, a), taken.room(width), stretch)?;
        if item.grow > 0.0 {
            let max = len_h(s.max_height, container.parent_h);
            size.height = clamp(size.height + item.grow, None, max).max(size.height);
        }
        // A float lower down may reach into it: then it fits the room
        // left there, or moves down.
        let over = taken.max(floats.band(top, size.height));
        if over != taken && size.width + a.margins() > over.room(width) + EPS {
            taken = over;
            continue;
        }
        break (over, size);
    };
    let Insets { left, .. } = taken;
    let (x, ml, mr) = a.place(item.align, left, taken.room(width), size.width);
    Ok(Geometry {
        x,
        y: top,
        w: size.width,
        h: size.height,
        margin: Edges {
            right: mr,
            left: ml,
            ..Edges::default()
        },
        ..Geometry::default()
    })
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
    let container = Container {
        tree,
        width,
        parent_h,
        collect,
        depth: 0,
    };
    layout_in(ctx, &container, idx, height, &[])
}

/// [`layout`] in `container`, beside the floats `outer` of the
/// containers around it.
fn layout_in(
    ctx: &mut LayoutCtx<'_>,
    container: &Container<'_, '_>,
    idx: NodeIdx,
    height: Option<f32>,
    outer: &[Placed],
) -> Result<FlowLayout, InkoxideError> {
    let (tree, width, parent_h) = (container.tree, container.width, container.parent_h);
    let mut placement = place(ctx, container, idx, Share::default(), outer)?;
    // The free room of a container whose height is known is shared out:
    // placed again with it, as floats may then sit beside other lines.
    if let Some(h) = height {
        let free = h - placement.bottom;
        let justify = tree.nodes[idx as usize].style().justify_content;
        let share = Share::of(&placement, free, justify);
        if free > EPS && share != Share::default() {
            placement = place(ctx, container, idx, share, outer)?;
        }
    }
    let Arrangement {
        mut placed,
        floats: float_ids,
        wraps,
        mut shared,
        bottom,
        ..
    } = placement;
    // What is set beside the floats around: lines, and boxes that start
    // above the lowest of them.
    let outer_bottom = outer.iter().map(Placed::bottom).fold(0.0, f32::max);
    let beside = placed
        .iter()
        .filter(|(_, g, _)| g.y < outer_bottom)
        .map(|(_, g, kind)| match *kind {
            Kind::Wrapped(set) => set.min(g.y + g.h),
            Kind::InFlow => g.y + g.h,
            Kind::Float(..) | Kind::Absolute => 0.0,
        })
        .fold(0.0, f32::max);
    let mut out = FlowLayout {
        wraps,
        beside,
        ..FlowLayout::default()
    };

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
                Kind::InFlow if g.y < fb && g.y + g.h > top => {
                    end = end.max(g.y + g.h);
                }
                _ => {}
            }
        }
        out.groups.push((f, end));
    }

    out.size = Size {
        width,
        height: bottom,
    };
    if !container.collect {
        return Ok(out);
    }
    let box_h = height.unwrap_or(bottom);
    let (pad, _) = insets(tree.nodes[idx as usize].style(), width);
    shared.reverse();
    for (k, (c, mut g, kind)) in placed.into_iter().enumerate() {
        if matches!(kind, Kind::Absolute) {
            let s = tree.nodes[c as usize].style();
            g = absolute(ctx, container, c, s, pad, box_h)?;
        }
        if matches!(kind, Kind::Wrapped(_)) {
            out.geos.push((c, g));
            // A box beside floats: its children, in its content box. Else
            // a text leaf, with no subtree.
            if shared.last().is_some_and(|(at, _)| *at == k) {
                let (_, fl) = shared.pop().expect("a box laid out beside floats");
                let (x, y) = (
                    g.x + g.border.left + g.padding.left,
                    g.y + g.border.top + g.padding.top,
                );
                out.geos
                    .extend(fl.geos.into_iter().map(|(n, ng)| (n, ng.offset(x, y))));
                out.groups
                    .extend(fl.groups.into_iter().map(|(f, end)| (f, y + end)));
            }
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
    /// Text wrapped beside floats, with the bottom of its last line set
    /// beside one.
    Wrapped(f32),
    /// A float, with the top and bottom of its margin box.
    Float(f32, f32),
    Absolute,
}

/// The border box of absolutely positioned child `c`, against the
/// container's padding box (`pad` around a content box `container.width`
/// × `height`).
fn absolute(
    ctx: &mut LayoutCtx<'_>,
    container: &Container<'_, '_>,
    c: NodeIdx,
    s: &Style,
    pad: Edges<f32>,
    height: f32,
) -> Result<Geometry, InkoxideError> {
    let (pw, ph) = (
        container.width + pad.left + pad.right,
        height + pad.top + pad.bottom,
    );
    let x_of = |l: Option<Length>| l.and_then(|l| len(l, pw));
    let y_of = |l: Option<Length>| l.and_then(|l| len(l, ph));
    let (left, right, top, bottom) = match s.inset {
        Some(i) => (x_of(i.left), x_of(i.right), y_of(i.top), y_of(i.bottom)),
        None => (None, None, None, None),
    };
    let a = Across::of(s, pw);
    let (mt, mb) = vertical_margins(s, pw);
    let [mt, mr, mb, ml] = [mt, a.right, mb, a.left].map(|m| m.unwrap_or(0.0));
    let w = match (a.width, left, right) {
        (Some(w), ..) => a.clamp(w),
        (None, Some(l), Some(r)) => a.clamp(pw - l - r - ml - mr),
        _ => {
            let room = pw - left.unwrap_or(0.0) - right.unwrap_or(0.0) - ml - mr;
            a.clamp(container.shrink_to_fit(ctx, c, room)?)
        }
    };
    let mode = RootMode::Item {
        width: AvailableSpace::Definite(w),
        fit: false,
        height: None,
        parent_h: Some(ph),
    };
    let mut h = ctx
        .layout_subtree(container.tree, c, mode, false)?
        .size
        .height;
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
    // A percentage of the width, unknown here, counts as nothing.
    let p = s.padding.unwrap_or_default().resolve(None);
    let pad = |v: Option<f32>| v.unwrap_or(0.0);
    let b = s.border_width.unwrap_or_default();
    let edges = pad(p.top) + pad(p.bottom) + b.top.unwrap_or(0.0) + b.bottom.unwrap_or(0.0);
    Some((h - edges).max(0.0))
}

/// The min-content or max-content width of float container `idx`. The
/// minimum is its widest child. The maximum is its widest in-flow child
/// with the floats beside it, or its floats side by side when wider; a
/// `clear` starts a new row of floats.
pub(super) fn intrinsic_width(
    ctx: &mut LayoutCtx<'_>,
    tree: &Tree<'_>,
    idx: NodeIdx,
    mode: AvailableSpace,
    parent_h: Option<f32>,
) -> Result<f32, InkoxideError> {
    let container = Container {
        tree,
        width: 0.0,
        parent_h,
        collect: false,
        depth: 0,
    };
    let max = mode == AvailableSpace::MaxContent;
    let (mut widest, mut row) = (0.0f32, 0.0f32);
    for c in tree.children(idx) {
        let Some(node) = tree.nodes[c as usize].node() else {
            continue;
        };
        let s = &node.style;
        if s.display == Some(Display::None) || s.position == Some(Position::Absolute) {
            continue;
        }
        let w = container.size(ctx, c, mode)?.width + Across::of(s, 0.0).margins();
        if s.clear.is_some_and(|c| c != Clear::None) {
            row = 0.0;
        }
        if s.floats().is_some() && max {
            row += w;
            widest = widest.max(row);
        } else if max {
            widest = widest.max(w + row);
        } else {
            widest = widest.max(w);
        }
    }
    Ok(widest)
}

/// Whether node `idx` is a column box with children, which `flow.rs`
/// lays out as Taffy does (tests check that): a box beside floats may
/// share them.
pub(super) fn column_box(tree: &Tree<'_>, idx: NodeIdx) -> bool {
    let ln = &tree.nodes[idx as usize];
    ln.holds_children()
        && !ln.children.is_empty()
        && ln.style().display != Some(Display::None)
        && ln.style().column()
}
