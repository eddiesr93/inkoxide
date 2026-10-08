//! Fast path for the most common table cell: one text child, no layout
//! styles beyond point padding and vertical alignment. Such a cell is a
//! column flexbox whose only item stretches to the content width, so its
//! layout is closed-form; building a Taffy tree per cell would dominate
//! the layout of large tables (ADR-028). Anything else goes to Taffy.

use taffy::prelude::{AvailableSpace, Size as TSize};

use super::{Geometry, LeafMeasure, NodeIdx, SubLayout, Tree};
use crate::schema::{
    AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, LengthPercent, NodeKind, Style,
    VerticalAlign,
};

/// The text child of cell `idx` and the cell's padding `[t, r, b, l]`,
/// when the cell qualifies.
pub(super) fn simple_cell(tree: &Tree<'_>, idx: NodeIdx) -> Option<(NodeIdx, [f32; 4])> {
    let ln = &tree.nodes[idx as usize];
    if ln.children.len() != 1 {
        return None;
    }
    let child = ln.children.start;
    let node = tree.nodes[child as usize].node?;
    if !matches!(node.kind, NodeKind::Text { .. }) || !plain_item(&node.style) {
        return None;
    }
    let s = ln.style();
    let container_ok = matches!(s.display, None | Some(Display::Flex))
        && matches!(s.flex_direction, None | Some(FlexDirection::Column))
        && matches!(s.flex_wrap, None | Some(FlexWrap::NoWrap))
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
    let mut pad = [0.0f32; 4];
    if let Some(p) = &s.padding {
        for (k, v) in [p.top, p.right, p.bottom, p.left].into_iter().enumerate() {
            match v {
                None => {}
                Some(LengthPercent::Points(x)) => pad[k] = x.max(0.0),
                Some(LengthPercent::Percent(_)) => return None,
            }
        }
    }
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
}

/// Lays out a qualifying cell. `None` when the closed form does not hold
/// (content taller than a fixed height, or narrower than the box edges).
#[allow(clippy::too_many_arguments)]
pub(super) fn cell(
    tree: &Tree<'_>,
    idx: NodeIdx,
    child: NodeIdx,
    pad: [f32; 4],
    leaves: &mut dyn LeafMeasure,
    width: AvailableSpace,
    height: Option<f32>,
    insets: [f32; 4],
    collect: bool,
) -> Option<SubLayout> {
    let node = tree.nodes[child as usize].node?;
    let edge_w = pad[1] + pad[3] + insets[1] + insets[3];
    let edge_h = pad[0] + pad[2] + insets[0] + insets[2];
    let (w, text) = match width {
        AvailableSpace::Definite(w) => {
            let inner = w - edge_w;
            if inner < 0.0 {
                return None;
            }
            let t = leaves.measure(
                node,
                TSize {
                    width: Some(inner),
                    height: None,
                },
                TSize {
                    width: AvailableSpace::Definite(inner),
                    height: AvailableSpace::MaxContent,
                },
            );
            (w, TSize { width: inner, ..t })
        }
        mode => {
            let t = leaves.measure(
                node,
                TSize::NONE,
                TSize {
                    width: mode,
                    height: AvailableSpace::MaxContent,
                },
            );
            (t.width + edge_w, t)
        }
    };
    let natural = text.height + edge_h;
    let h = match height {
        Some(h) if h + 1e-3 < natural => return None,
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
            },
        ));
        geos.push((
            child,
            Geometry {
                x: insets[3] + pad[3],
                y: insets[0] + pad[0] + dy,
                w: text.width,
                h: text.height,
                ..Default::default()
            },
        ));
    }
    Some(SubLayout {
        size: TSize {
            width: w,
            height: h,
        },
        geos,
    })
}
