//! Layout: schema tree → arena of absolutely positioned boxes.
//!
//! The arena keeps nodes in breadth-first order so every node's children
//! occupy a contiguous index range. Tables appear in the arena as
//! table → rows → cells → cell content. Taffy computes flex/grid boxes;
//! tables are Taffy leaves sized by our own algorithm (`table.rs`), and
//! each cell is laid out as its own small Taffy subtree. Taffy trees are
//! dropped afterwards; the rest of the pipeline only reads the arena.

// Grid code walks parallel arrays by index; iterators would obscure it.
#![allow(clippy::needless_range_loop)]

mod fast;
mod style;
pub(crate) mod table;

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

use taffy::prelude::{AvailableSpace, NodeId as TaffyId, Size as TSize, TaffyTree};
use taffy::{LayoutInput, LayoutOutput};

pub(crate) use table::TableLayout;

use crate::schema::{Node, NodeKind, Style, TableCell, TableRow, VerticalAlign};
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

    fn offset(mut self, dx: f32, dy: f32) -> Self {
        self.x += dx;
        self.y += dy;
        self
    }
}

/// What an arena node stands for.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Part<'a> {
    /// The synthetic root (the page content box, or a header/footer band).
    Root,
    Node(&'a Node),
    /// A table row.
    Row(&'a TableRow),
    Cell(&'a TableCell),
}

#[derive(Debug)]
pub(crate) struct LNode<'a> {
    /// The document node, when this is one (not a root, row or cell).
    pub node: Option<&'a Node>,
    pub part: Part<'a>,
    pub parent: NodeIdx,
    pub children: Range<NodeIdx>,
    pub geo: Geometry,
}

impl<'a> LNode<'a> {
    pub fn style(&self) -> &'a Style {
        static DEFAULT: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
        match self.part {
            Part::Node(n) => &n.style,
            Part::Cell(c) => &c.style,
            Part::Root | Part::Row(..) => DEFAULT.get_or_init(Style::default),
        }
    }

    pub fn is_table(&self) -> bool {
        matches!(self.node.map(|n| &n.kind), Some(NodeKind::Table(_)))
    }
}

/// Arena of layout nodes. Index 0 is the root.
#[derive(Debug, Default)]
pub(crate) struct Tree<'a> {
    pub nodes: Vec<LNode<'a>>,
    /// Final layouts of the tables in this tree, by table node.
    pub tables: BTreeMap<NodeIdx, Arc<TableLayout>>,
}

impl<'a> Tree<'a> {
    /// Builds the arena for a flow (`root_children` under a synthetic root).
    pub fn build(root_children: &'a [Node]) -> Self {
        let mut nodes = vec![LNode {
            node: None,
            part: Part::Root,
            parent: 0,
            children: 0..0,
            geo: Geometry::default(),
        }];
        let new = |part: Part<'a>, parent: usize| LNode {
            node: match part {
                Part::Node(n) => Some(n),
                _ => None,
            },
            part,
            parent: parent as NodeIdx,
            children: 0..0,
            geo: Geometry::default(),
        };
        // Breadth-first: children of node i are appended together.
        let mut i = 0;
        while i < nodes.len() {
            let start = nodes.len() as NodeIdx;
            match nodes[i].part {
                Part::Root => nodes.extend(root_children.iter().map(|k| new(Part::Node(k), i))),
                Part::Node(n) => match &n.kind {
                    NodeKind::Box { children } => {
                        nodes.extend(children.iter().map(|k| new(Part::Node(k), i)));
                    }
                    NodeKind::Table(t) => {
                        nodes.extend(t.rows.iter().map(|r| new(Part::Row(r), i)));
                    }
                    _ => {}
                },
                Part::Row(r) => nodes.extend(r.cells.iter().map(|c| new(Part::Cell(c), i))),
                Part::Cell(c) => nodes.extend(c.children.iter().map(|k| new(Part::Node(k), i))),
            }
            nodes[i].children = start..nodes.len() as NodeIdx;
            i += 1;
        }
        Tree {
            nodes,
            tables: BTreeMap::new(),
        }
    }

    /// Builds the arena for a single node (header/footer).
    pub fn build_single(node: &'a Node) -> Self {
        Self::build(std::slice::from_ref(node))
    }

    pub fn children(&self, i: NodeIdx) -> impl DoubleEndedIterator<Item = NodeIdx> {
        self.nodes[i as usize].children.clone()
    }
}

/// Measures leaf content (text, images) for the layout engine.
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
#[cfg(test)]
pub(crate) struct NoContent;

#[cfg(test)]
impl LeafMeasure for NoContent {
    fn measure(&mut self, _: &Node, _: TSize<Option<f32>>, _: TSize<AvailableSpace>) -> TSize<f32> {
        TSize::ZERO
    }
}

/// Dependency firewall around the flex/grid engine (§5.1).
pub(crate) trait LayoutEngine {
    /// Lays out `tree` in a column of the given `width` and unbounded
    /// height (at least `min_height`, so `flexGrow` can fill a short page),
    /// writing absolute geometry into every node (tables included).
    fn compute(
        &mut self,
        tree: &mut Tree<'_>,
        width: f32,
        min_height: f32,
        measure: &mut dyn LeafMeasure,
    ) -> Result<(), InkoxideError>;
}

pub(crate) struct TaffyLayout;

#[expect(clippy::needless_pass_by_value, reason = "a `map_err` adapter")]
fn layout_err(e: taffy::TaffyError) -> InkoxideError {
    InkoxideError::Layout(e.to_string())
}

/// How the root of a subtree layout is sized.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RootMode {
    /// The synthetic flow root: a column of fixed width.
    Flow { width: f32, min_height: f32 },
    /// A table cell: `width` is its border-box width (or a content-sizing
    /// mode), `height` its final border-box height when known. `insets`
    /// are the collapsed border widths reserved inside the cell.
    Cell {
        width: AvailableSpace,
        height: Option<f32>,
        insets: [f32; 4],
    },
}

/// Shared state of one layout run: leaf measurement plus table caches.
pub(crate) struct LayoutCtx<'m> {
    pub leaves: &'m mut dyn LeafMeasure,
    /// Table layouts by (table node, width key).
    tables: HashMap<(NodeIdx, u32), Arc<TableLayout>>,
    /// Cell sizes by (cell node, width key, height key).
    cells: HashMap<(NodeIdx, u32), TSize<f32>>,
    /// Closed-form layout of simple cells (`fast.rs`); off only in tests.
    pub fast_cells: bool,
}

fn space_key(a: AvailableSpace) -> u32 {
    match a {
        AvailableSpace::Definite(w) => w.max(0.0).to_bits() & !3,
        AvailableSpace::MinContent => 1,
        AvailableSpace::MaxContent => 2,
    }
}

/// Result of laying out a subtree: geometry relative to the subtree root's
/// top-left corner, for the root and each descendant (tables are leaves).
pub(crate) struct SubLayout {
    pub size: TSize<f32>,
    pub geos: Vec<(NodeIdx, Geometry)>,
}

impl<'m> LayoutCtx<'m> {
    pub fn new(leaves: &'m mut dyn LeafMeasure) -> Self {
        Self {
            leaves,
            tables: HashMap::new(),
            cells: HashMap::new(),
            fast_cells: true,
        }
    }

    /// Lays out the subtree under `root` with Taffy.
    pub fn layout_subtree(
        &mut self,
        tree: &Tree<'_>,
        root: NodeIdx,
        mode: RootMode,
        collect: bool,
    ) -> Result<SubLayout, InkoxideError> {
        if let (
            true,
            RootMode::Cell {
                width,
                height,
                insets,
            },
        ) = (self.fast_cells, mode)
        {
            if let Some((child, pad)) = fast::simple_cell(tree, root) {
                if let Some(sub) = fast::cell(
                    tree,
                    root,
                    child,
                    pad,
                    self.leaves,
                    width,
                    height,
                    insets,
                    collect,
                ) {
                    return Ok(sub);
                }
            }
        }
        // Subtree nodes in BFS order; children of each entry are contiguous.
        // Table internals are excluded: tables are leaves here.
        let mut list: Vec<(NodeIdx, usize, usize)> = vec![(root, 0, 0)];
        let mut k = 0;
        while k < list.len() {
            let idx = list[k].0;
            let ln = &tree.nodes[idx as usize];
            let leaf_table = ln.is_table();
            let start = list.len();
            if !leaf_table {
                list.extend(ln.children.clone().map(|c| (c, 0, 0)));
            }
            list[k].1 = start;
            list[k].2 = list.len() - start;
            k += 1;
        }

        let mut taffy: TaffyTree<NodeIdx> = TaffyTree::with_capacity(list.len());
        taffy.disable_rounding();
        let mut ids: Vec<TaffyId> = vec![TaffyId::from(0u64); list.len()];
        for k in (0..list.len()).rev() {
            let (idx, start, len) = list[k];
            let ln = &tree.nodes[idx as usize];
            let kids = &ids[start..start + len];
            let id = if k == 0 {
                taffy.new_with_children(root_style(ln, mode), kids)
            } else {
                let s = style::to_taffy(ln.style());
                if len == 0 {
                    taffy.new_leaf_with_context(s, idx)
                } else {
                    taffy.new_with_children(s, kids)
                }
            }
            .map_err(layout_err)?;
            ids[k] = id;
        }

        let available = TSize {
            width: match mode {
                RootMode::Flow { width, .. } => AvailableSpace::Definite(width),
                RootMode::Cell { width, .. } => width,
            },
            height: AvailableSpace::MaxContent,
        };
        let nodes = &tree.nodes;
        taffy
            .compute_layout_with_measure(
                ids[0],
                available,
                |inputs: LayoutInput,
                 _id,
                 ctx: Option<&mut NodeIdx>,
                 style: &taffy::Style|
                 -> LayoutOutput {
                    let Some(&mut idx) = ctx else {
                        return taffy::compute_leaf_layout(
                            inputs,
                            style,
                            |_, _| 0.0,
                            |_, _| TSize::ZERO,
                        );
                    };
                    taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| match nodes[idx as usize].node {
                            Some(n) if matches!(n.kind, NodeKind::Table(_)) => {
                                self.table_size(tree, idx, known, available)
                            }
                            Some(n) => self.leaves.measure(n, known, available),
                            None => TSize::ZERO,
                        },
                    )
                },
            )
            .map_err(layout_err)?;

        let root_layout = taffy.layout(ids[0]).map_err(layout_err)?;
        let size = root_layout.size;
        let mut geos = Vec::new();
        if collect {
            // Positions are relative to the parent; accumulate from the root.
            let mut abs: Vec<(f32, f32)> = vec![(0.0, 0.0); list.len()];
            let mut parent_pos: Vec<usize> = vec![0; list.len()];
            for k in 0..list.len() {
                let (_, start, len) = list[k];
                for c in start..start + len {
                    parent_pos[c] = k;
                }
            }
            geos.reserve(list.len());
            for k in 0..list.len() {
                let l = taffy.layout(ids[k]).map_err(layout_err)?;
                let (px, py) = if k == 0 {
                    (0.0, 0.0)
                } else {
                    abs[parent_pos[k]]
                };
                let (x, y) = if k == 0 {
                    (0.0, 0.0)
                } else {
                    (px + l.location.x, py + l.location.y)
                };
                abs[k] = (x, y);
                geos.push((
                    list[k].0,
                    Geometry {
                        x,
                        y,
                        w: l.size.width,
                        h: l.size.height,
                        border: [l.border.top, l.border.right, l.border.bottom, l.border.left],
                        padding: [
                            l.padding.top,
                            l.padding.right,
                            l.padding.bottom,
                            l.padding.left,
                        ],
                    },
                ));
            }
        }
        Ok(SubLayout { size, geos })
    }

    /// Size of table leaf `idx` for Taffy.
    fn table_size(
        &mut self,
        tree: &Tree<'_>,
        idx: NodeIdx,
        _known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32> {
        // `known` is the border box; `available` is already the content
        // box (Taffy subtracts padding and border from a known width).
        match self.table(tree, idx, available.width) {
            Ok(t) => TSize {
                width: t.width,
                height: t.height,
            },
            Err(e) => {
                tracing::warn!("table layout failed: {e}");
                TSize::ZERO
            }
        }
    }

    /// Table layout of table node `idx` for an available content width.
    pub fn table(
        &mut self,
        tree: &Tree<'_>,
        idx: NodeIdx,
        space: AvailableSpace,
    ) -> Result<Arc<TableLayout>, InkoxideError> {
        let key = (idx, space_key(space));
        if let Some(t) = self.tables.get(&key) {
            return Ok(t.clone());
        }
        let t = Arc::new(table::compute(self, tree, idx, space)?);
        self.tables.insert(key, t.clone());
        Ok(t)
    }

    /// Border-box size of cell `idx` at a width (or content-sizing mode).
    pub fn cell_size(
        &mut self,
        tree: &Tree<'_>,
        idx: NodeIdx,
        width: AvailableSpace,
        insets: [f32; 4],
    ) -> Result<TSize<f32>, InkoxideError> {
        let key = (idx, space_key(width));
        if let Some(s) = self.cells.get(&key) {
            return Ok(*s);
        }
        let s = self
            .layout_subtree(
                tree,
                idx,
                RootMode::Cell {
                    width,
                    height: None,
                    insets,
                },
                false,
            )?
            .size;
        self.cells.insert(key, s);
        Ok(s)
    }
}

/// Taffy style of a subtree root.
fn root_style(ln: &LNode<'_>, mode: RootMode) -> taffy::Style {
    use taffy::style::{Dimension, JustifyContent, LengthPercentage, LengthPercentageAuto};
    match mode {
        RootMode::Flow { width, min_height } => taffy::Style {
            flex_direction: taffy::style::FlexDirection::Column,
            size: TSize {
                width: Dimension::length(width),
                height: Dimension::auto(),
            },
            min_size: TSize {
                width: LengthPercentageAuto::auto(),
                height: LengthPercentageAuto::length(min_height),
            },
            ..Default::default()
        },
        RootMode::Cell {
            width,
            height,
            insets,
        } => {
            let mut s = style::to_taffy(ln.style());
            // Collapsed borders are reserved as Taffy borders (the table
            // paints them).
            let [t, r, b, l] = insets.map(LengthPercentage::length);
            s.border = taffy::geometry::Rect {
                top: t,
                right: r,
                bottom: b,
                left: l,
            };
            s.margin = taffy::geometry::Rect::zero();
            s.size.width = match width {
                AvailableSpace::Definite(w) => Dimension::length(w),
                _ => Dimension::auto(),
            };
            if let Some(h) = height {
                s.size.height = Dimension::length(h);
                s.min_size.height = LengthPercentageAuto::length(h);
                s.max_size.height = LengthPercentageAuto::length(h);
            }
            s.justify_content = Some(match ln.style().vertical_align {
                Some(VerticalAlign::Middle) => JustifyContent::CENTER,
                Some(VerticalAlign::Bottom) => JustifyContent::FLEX_END,
                _ => JustifyContent::FLEX_START,
            });
            s
        }
    }
}

impl LayoutEngine for TaffyLayout {
    fn compute(
        &mut self,
        tree: &mut Tree<'_>,
        width: f32,
        min_height: f32,
        measure: &mut dyn LeafMeasure,
    ) -> Result<(), InkoxideError> {
        compute_tree(tree, width, min_height, measure, true)
    }
}

/// [`TaffyLayout::compute`], with the simple-cell fast path switchable so
/// tests can check it against Taffy.
pub(crate) fn compute_tree(
    tree: &mut Tree<'_>,
    width: f32,
    min_height: f32,
    measure: &mut dyn LeafMeasure,
    fast_cells: bool,
) -> Result<(), InkoxideError> {
    let mut ctx = LayoutCtx::new(measure);
    ctx.fast_cells = fast_cells;
    let sub = ctx.layout_subtree(tree, 0, RootMode::Flow { width, min_height }, true)?;
    for (idx, g) in sub.geos {
        tree.nodes[idx as usize].geo = g;
    }

    // Tables, outer before nested (BFS order): lay out rows and cells at
    // the final width and write their absolute geometry.
    let mut i = 0usize;
    while i < tree.nodes.len() {
        if tree.nodes[i].is_table() {
            let idx = i as NodeIdx;
            let (cx, cy, cw, _) = tree.nodes[i].geo.content_box();
            let layout = ctx.table(tree, idx, AvailableSpace::Definite(cw))?;
            let rows: Vec<NodeIdx> = tree.children(idx).collect();
            for (ri, &row) in rows.iter().enumerate() {
                tree.nodes[row as usize].geo = Geometry {
                    x: cx,
                    y: cy + layout.row_y[ri],
                    w: layout.width,
                    h: layout.row_h[ri],
                    ..Default::default()
                };
            }
            for cell in &layout.cells {
                let sub = ctx.layout_subtree(
                    tree,
                    cell.idx,
                    RootMode::Cell {
                        width: AvailableSpace::Definite(cell.w),
                        height: Some(cell.h),
                        insets: cell.insets,
                    },
                    true,
                )?;
                let (ox, oy) = (cx + cell.x, cy + cell.y);
                for (n, g) in sub.geos {
                    tree.nodes[n as usize].geo = g.offset(ox, oy);
                }
            }
            tree.tables.insert(idx, layout);
        }
        i += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
