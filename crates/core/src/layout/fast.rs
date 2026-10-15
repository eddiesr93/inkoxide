//! Fast path for the most common table cell: one text child, no layout
//! styles beyond point padding and vertical alignment. Such a cell is a
//! column flexbox whose only item stretches to the content width, so its
//! layout is closed-form; building a Taffy tree per cell would dominate
//! the layout of large tables. Anything else goes to Taffy.

use super::{AvailableSpace, CellSpace, Geometry, LeafMeasure, NodeIdx, Size, SubLayout, Tree};
use crate::schema::{
    AlignItems, AlignSelf, Display, Edges, LengthPercent, NodeKind, Style, VerticalAlign,
};

/// A set height short of the content by less than this (points) is float
/// noise: the content still fits.
const HEIGHT_EPS: f32 = 1e-3;

/// The text child of cell `idx` and the cell's padding, when the cell
/// qualifies.
pub(super) fn simple_cell(tree: &Tree<'_>, idx: NodeIdx) -> Option<(NodeIdx, Edges<f32>)> {
    let ln = &tree.nodes[idx as usize];
    if ln.children.len() != 1 {
        return None;
    }
    let child = ln.children.start;
    let node = tree.nodes[child as usize].node()?;
    if !matches!(node.kind, NodeKind::Text { .. }) || !plain_item(&node.style) {
        return None;
    }
    let s = ln.style();
    let container_ok = s.display != Some(Display::None)
        && s.column()
        && matches!(s.align_items, None | Some(AlignItems::Stretch))
        && s.align_content.is_none()
        && s.height.is_none()
        && s.min_width.is_none()
        && s.min_height.is_none()
        && s.max_width.is_none()
        && s.max_height.is_none()
        && s.aspect_ratio.is_none()
        && s.position.is_none()
        && s.inset.is_none();
    if !container_ok {
        return None;
    }
    let side = |v: Option<LengthPercent>| match v {
        None => Some(0.0),
        Some(LengthPercent::Points(x)) => Some(x.max(0.0)),
        Some(LengthPercent::Percent(_)) => None,
    };
    let pad = match &s.padding {
        Some(p) => Edges {
            top: side(p.top)?,
            right: side(p.right)?,
            bottom: side(p.bottom)?,
            left: side(p.left)?,
        },
        None => Edges::default(),
    };
    Some((child, pad))
}

/// A flex item with no box of its own: it takes exactly its measured size.
fn plain_item(s: &Style) -> bool {
    matches!(s.display, None | Some(Display::Flex))
        && matches!(
            s.align_self,
            None | Some(AlignSelf::Auto | AlignSelf::Stretch)
        )
        && s.flex_grow.is_none()
        && s.flex_shrink.is_none()
        && s.flex_basis.is_none()
        && s.width.is_none()
        && s.height.is_none()
        && s.min_width.is_none()
        && s.min_height.is_none()
        && s.max_width.is_none()
        && s.max_height.is_none()
        && s.aspect_ratio.is_none()
        && s.padding.is_none()
        && s.margin.is_none()
        && s.position.is_none()
        && s.inset.is_none()
        && s.border_width.is_none()
        && s.float.is_none()
        && s.clear.is_none()
}

/// Lays out a qualifying cell. `None` when the closed form does not hold
/// (content taller than a fixed height, or narrower than the box edges).
pub(super) fn cell(
    tree: &Tree<'_>,
    idx: NodeIdx,
    child: NodeIdx,
    pad: Edges<f32>,
    leaves: &mut dyn LeafMeasure,
    space: CellSpace,
    collect: bool,
) -> Option<SubLayout> {
    let CellSpace {
        width,
        height,
        insets,
    } = space;
    let node = tree.nodes[child as usize].node()?;
    let edge_w = pad.right + pad.left + insets.right + insets.left;
    let edge_h = pad.top + pad.bottom + insets.top + insets.bottom;
    let (w, text) = match width {
        AvailableSpace::Definite(w) => {
            let inner = w - edge_w;
            if inner < 0.0 {
                return None;
            }
            let t = leaves.measure(
                node,
                Size {
                    width: Some(inner),
                    height: None,
                },
                Size {
                    width: AvailableSpace::Definite(inner),
                    height: AvailableSpace::MaxContent,
                },
            );
            (w, Size { width: inner, ..t })
        }
        mode => {
            let t = leaves.measure(
                node,
                Size::NONE,
                Size {
                    width: mode,
                    height: AvailableSpace::MaxContent,
                },
            );
            (t.width + edge_w, t)
        }
    };
    let natural = text.height + edge_h;
    let h = match height {
        Some(h) if h + HEIGHT_EPS < natural => return None,
        Some(h) => h,
        None => natural,
    };
    let mut geos = Vec::new();
    if collect {
        let s = tree.nodes[idx as usize].style();
        let free = h - natural;
        let dy = match s.vertical_align {
            Some(VerticalAlign::Middle) => free / 2.0,
            Some(VerticalAlign::Bottom) => free,
            _ => 0.0,
        };
        geos.push((
            idx,
            Geometry {
                x: 0.0,
                y: 0.0,
                w,
                h,
                border: insets,
                padding: pad,
                margin: Edges::default(),
            },
        ));
        geos.push((
            child,
            Geometry {
                x: insets.left + pad.left,
                y: insets.top + pad.top + dy,
                w: text.width,
                h: text.height,
                ..Default::default()
            },
        ));
    }
    Some(SubLayout {
        size: Size {
            width: w,
            height: h,
        },
        geos,
    })
}
