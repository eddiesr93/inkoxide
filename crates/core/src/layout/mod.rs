//! Layout: schema tree → arena of absolutely positioned boxes.
//!
//! The arena keeps nodes in breadth-first order so every node's children
//! occupy a contiguous index range. Taffy computes the boxes; its tree is
//! dropped afterwards and the rest of the pipeline only reads the arena.

mod style;

use std::ops::Range;

use taffy::prelude::{AvailableSpace, NodeId as TaffyId, Size as TSize, TaffyTree};
use taffy::{LayoutInput, LayoutOutput};

use crate::schema::{Node, NodeKind, Style};
use crate::InkoxideError;

pub(crate) type NodeIdx = u32;

/// Absolute geometry of a node, in flow coordinates (origin at the top-left
/// of the content box of the page template, y down).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Geometry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Border widths `[top, right, bottom, left]` as resolved by layout.
    pub border: [f32; 4],
    /// Padding `[top, right, bottom, left]` as resolved by layout.
    pub padding: [f32; 4],
}

impl Geometry {
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Content box `(x, y, w, h)`.
    pub fn content_box(&self) -> (f32, f32, f32, f32) {
        let [t, r, b, l] = self.border;
        let [pt, pr, pb, pl] = self.padding;
        (
            self.x + l + pl,
            self.y + t + pt,
            (self.w - l - r - pl - pr).max(0.0),
            (self.h - t - b - pt - pb).max(0.0),
        )
    }
}

#[derive(Debug)]
pub(crate) struct LNode<'a> {
    /// `None` for the synthetic root (the page content box).
    pub node: Option<&'a Node>,
    pub parent: NodeIdx,
    pub children: Range<NodeIdx>,
    pub geo: Geometry,
}

impl<'a> LNode<'a> {
    pub fn style(&self) -> &'a Style {
        static DEFAULT: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
        match self.node {
            Some(n) => &n.style,
            None => DEFAULT.get_or_init(Style::default),
        }
    }
}

/// Arena of layout nodes. Index 0 is the root.
#[derive(Debug, Default)]
pub(crate) struct Tree<'a> {
    pub nodes: Vec<LNode<'a>>,
}

impl<'a> Tree<'a> {
    /// Builds the arena for a flow (`root_children` under a synthetic root).
    pub fn build(root_children: &'a [Node]) -> Self {
        let mut nodes = vec![LNode {
            node: None,
            parent: 0,
            children: 0..0,
            geo: Geometry::default(),
        }];
        // Breadth-first: children of node i are appended together.
        let mut i = 0;
        while i < nodes.len() {
            let kids: &'a [Node] = match nodes[i].node {
                None => root_children,
                Some(n) => match &n.kind {
                    NodeKind::Box { children } => children,
                    _ => &[],
                },
            };
            let start = nodes.len() as NodeIdx;
            nodes.extend(kids.iter().map(|k| LNode {
                node: Some(k),
                parent: i as NodeIdx,
                children: 0..0,
                geo: Geometry::default(),
            }));
            nodes[i].children = start..nodes.len() as NodeIdx;
            i += 1;
        }
        Tree { nodes }
    }

    /// Builds the arena for a single node (header/footer).
    pub fn build_single(node: &'a Node) -> Self {
        Self::build(std::slice::from_ref(node))
    }

    pub fn children(&self, i: NodeIdx) -> impl Iterator<Item = NodeIdx> {
        self.nodes[i as usize].children.clone()
    }
}

/// Measures leaf content (text, images, tables) for the layout engine.
pub(crate) trait LeafMeasure {
    /// Size of leaf `node` given known dimensions and available space.
    fn measure(
        &mut self,
        node: &Node,
        known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32>;
}

/// Leaves with no intrinsic content (empty boxes, spacers, page breaks).
pub(crate) struct NoContent;

impl LeafMeasure for NoContent {
    fn measure(&mut self, _: &Node, _: TSize<Option<f32>>, _: TSize<AvailableSpace>) -> TSize<f32> {
        TSize::ZERO
    }
}

/// Dependency firewall around the flex/grid engine (§5.1).
pub(crate) trait LayoutEngine {
    /// Lays out `tree` in a column of the given `width` and unbounded
    /// height, writing absolute geometry into every node.
    fn compute(
        &mut self,
        tree: &mut Tree<'_>,
        width: f32,
        measure: &mut dyn LeafMeasure,
    ) -> Result<(), InkoxideError>;
}

pub(crate) struct TaffyLayout;

fn layout_err(e: taffy::TaffyError) -> InkoxideError {
    InkoxideError::Layout(e.to_string())
}

impl LayoutEngine for TaffyLayout {
    fn compute(
        &mut self,
        tree: &mut Tree<'_>,
        width: f32,
        measure: &mut dyn LeafMeasure,
    ) -> Result<(), InkoxideError> {
        let mut taffy: TaffyTree<NodeIdx> = TaffyTree::with_capacity(tree.nodes.len());
        taffy.disable_rounding();

        // Children have larger indices than their parent (BFS), so building
        // back to front always finds the children already created.
        let mut ids: Vec<TaffyId> = vec![TaffyId::from(0u64); tree.nodes.len()];
        for i in (0..tree.nodes.len()).rev() {
            let n = &tree.nodes[i];
            let id = if i == 0 {
                let root_style = taffy::Style {
                    flex_direction: taffy::style::FlexDirection::Column,
                    size: TSize {
                        width: taffy::style::Dimension::length(width),
                        height: taffy::style::Dimension::auto(),
                    },
                    ..Default::default()
                };
                let kids: Vec<TaffyId> = n.children.clone().map(|c| ids[c as usize]).collect();
                taffy.new_with_children(root_style, &kids)
            } else {
                let s = style::to_taffy(n.style());
                if n.children.is_empty() {
                    taffy.new_leaf_with_context(s, i as NodeIdx)
                } else {
                    let kids: Vec<TaffyId> = n.children.clone().map(|c| ids[c as usize]).collect();
                    taffy.new_with_children(s, &kids)
                }
            }
            .map_err(layout_err)?;
            ids[i] = id;
        }

        let nodes = &tree.nodes;
        taffy
            .compute_layout_with_measure(
                ids[0],
                TSize {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::MaxContent,
                },
                |inputs: LayoutInput,
                 _id,
                 ctx: Option<&mut NodeIdx>,
                 style: &taffy::Style|
                 -> LayoutOutput {
                    let node = ctx.and_then(|i| nodes[*i as usize].node);
                    taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| match node {
                            Some(n) => measure.measure(n, known, available),
                            None => TSize::ZERO,
                        },
                    )
                },
            )
            .map_err(layout_err)?;

        // Copy absolute geometry into the arena (parents before children).
        for (i, id) in ids.iter().enumerate() {
            let l = taffy.layout(*id).map_err(layout_err)?;
            let (px, py) = if i == 0 {
                (0.0, 0.0)
            } else {
                let p = &tree.nodes[tree.nodes[i].parent as usize].geo;
                (p.x, p.y)
            };
            tree.nodes[i].geo = Geometry {
                x: px + l.location.x,
                y: py + l.location.y,
                w: l.size.width,
                h: l.size.height,
                border: [l.border.top, l.border.right, l.border.bottom, l.border.left],
                padding: [
                    l.padding.top,
                    l.padding.right,
                    l.padding.bottom,
                    l.padding.left,
                ],
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
