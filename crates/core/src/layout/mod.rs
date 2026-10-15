//! Layout: schema tree → arena of absolutely positioned boxes.
//!
//! The arena keeps nodes in breadth-first order so every node's children
//! occupy a contiguous index range. Tables appear in the arena as
//! table → rows → cells → cell content. Taffy computes flex/grid boxes;
//! tables are Taffy leaves sized by our own algorithm (`table.rs`), and
//! each cell is laid out as its own small Taffy subtree, or in closed form
//! for simple text cells (`fast.rs`). Taffy trees are dropped afterwards;
//! the rest of the pipeline only reads the arena. Taffy's types stay in
//! this module: leaves are measured with [`Size`] and [`AvailableSpace`].

// Grid code walks parallel arrays by index; iterators would obscure it.
#![allow(clippy::needless_range_loop)]

mod fast;
pub(crate) mod float;
mod flow;
mod style;
pub(crate) mod table;

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

use taffy::prelude::{NodeId as TaffyId, Size as TSize, TaffyTree};
use taffy::{LayoutInput, LayoutOutput};

pub(crate) use float::LineBoxes;
pub(crate) use flow::Wrapped;
pub(crate) use table::TableLayout;

use crate::paint::Rect;
use crate::schema::{Edges, FieldKind, Node, NodeKind, Style, TableCell, TableRow, VerticalAlign};
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
    /// Border widths as resolved by layout.
    pub border: Edges<f32>,
    /// Padding as resolved by layout.
    pub padding: Edges<f32>,
    /// Margins as resolved by layout (`auto` included). Only `debug`
    /// outlines read them.
    pub margin: Edges<f32>,
}

impl Geometry {
    /// The border box less border and padding.
    pub fn content_box(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
            .inset(self.border)
            .inset(self.padding)
    }

    fn offset(mut self, dx: f32, dy: f32) -> Self {
        self.x += dx;
        self.y += dy;
        self
    }
}

/// A width and a height.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Size<T> {
    pub width: T,
    pub height: T,
}

impl Size<f32> {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };
}

impl Size<Option<f32>> {
    pub const NONE: Self = Self {
        width: None,
        height: None,
    };
}

impl<T> From<TSize<T>> for Size<T> {
    fn from(s: TSize<T>) -> Self {
        Self {
            width: s.width,
            height: s.height,
        }
    }
}

impl<T> From<Size<T>> for TSize<T> {
    fn from(s: Size<T>) -> Self {
        Self {
            width: s.width,
            height: s.height,
        }
    }
}

/// The space a leaf is measured in, along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AvailableSpace {
    /// A definite size, in points.
    Definite(f32),
    /// The narrowest the content can be.
    MinContent,
    /// The size the content takes unconstrained.
    MaxContent,
}

impl From<taffy::AvailableSpace> for AvailableSpace {
    fn from(a: taffy::AvailableSpace) -> Self {
        match a {
            taffy::AvailableSpace::Definite(w) => Self::Definite(w),
            taffy::AvailableSpace::MinContent => Self::MinContent,
            taffy::AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

impl From<AvailableSpace> for taffy::AvailableSpace {
    fn from(a: AvailableSpace) -> Self {
        match a {
            AvailableSpace::Definite(w) => Self::Definite(w),
            AvailableSpace::MinContent => Self::MinContent,
            AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

/// What an arena node stands for.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Part<'a> {
    /// The synthetic root (the page content box, or the box a header,
    /// footer or overlay is laid out in).
    Root,
    Node(&'a Node),
    /// A table row.
    Row(&'a TableRow),
    Cell(&'a TableCell),
}

#[derive(Debug)]
pub(crate) struct LNode<'a> {
    pub part: Part<'a>,
    pub parent: NodeIdx,
    pub children: Range<NodeIdx>,
    pub geo: Geometry,
}

impl<'a> LNode<'a> {
    /// The document node, when this is one (not a root, row or cell).
    pub fn node(&self) -> Option<&'a Node> {
        match self.part {
            Part::Node(n) => Some(n),
            _ => None,
        }
    }

    pub fn style(&self) -> &'a Style {
        static DEFAULT: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
        match self.part {
            Part::Node(n) => &n.style,
            Part::Cell(c) => &c.style,
            Part::Root | Part::Row(..) => DEFAULT.get_or_init(Style::default),
        }
    }

    pub fn is_table(&self) -> bool {
        matches!(self.node().map(|n| &n.kind), Some(NodeKind::Table(_)))
    }

    /// Whether it lays out children as a box does: the root, a box or a
    /// table cell (not a row, nor a leaf).
    pub fn holds_children(&self) -> bool {
        match self.part {
            Part::Root | Part::Cell(_) => true,
            Part::Node(n) => matches!(n.kind, NodeKind::Box { .. }),
            Part::Row(_) => false,
        }
    }
}

/// Arena of layout nodes. Index 0 is the root.
#[derive(Debug, Default)]
pub(crate) struct Tree<'a> {
    pub nodes: Vec<LNode<'a>>,
    /// Final layouts of the tables in this tree, by table node.
    pub tables: BTreeMap<NodeIdx, Arc<TableLayout>>,
    /// Per node: whether it holds floats (`flow.rs` lays out its children).
    pub floats: Vec<bool>,
    /// The line boxes of text laid out beside floats, by text node.
    pub wraps: BTreeMap<NodeIdx, Arc<LineBoxes>>,
    /// Per float: the flow y where its group ends (the floats beside it,
    /// and the lines and boxes beside them), which moves to a new page
    /// with it.
    pub float_groups: BTreeMap<NodeIdx, f32>,
}

impl<'a> Tree<'a> {
    /// Builds the arena for a flow (`root_children` under a synthetic root).
    pub fn build(root_children: impl IntoIterator<Item = &'a Node>) -> Self {
        let mut root_children = Some(root_children.into_iter());
        let mut nodes = vec![LNode {
            part: Part::Root,
            parent: 0,
            children: 0..0,
            geo: Geometry::default(),
        }];
        let new = |part: Part<'a>, parent: usize| LNode {
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
                Part::Root => {
                    if let Some(kids) = root_children.take() {
                        nodes.extend(kids.map(|k| new(Part::Node(k), i)));
                    }
                }
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
        let mut tree = Tree {
            nodes,
            tables: BTreeMap::new(),
            floats: Vec::new(),
            wraps: BTreeMap::new(),
            float_groups: BTreeMap::new(),
        };
        tree.floats = (0..tree.nodes.len())
            .map(|i| flow::holds_floats(&tree, i))
            .collect();
        tree
    }

    /// Builds the arena for a single node (a header, footer or overlay).
    pub fn build_single(node: &'a Node) -> Self {
        Self::build(std::slice::from_ref(node))
    }

    pub fn children(&self, i: NodeIdx) -> impl DoubleEndedIterator<Item = NodeIdx> {
        self.nodes[i as usize].children.clone()
    }

    /// Whether node `i` holds floats.
    pub fn holds_floats(&self, i: NodeIdx) -> bool {
        self.floats.get(i as usize).copied().unwrap_or(false)
    }

    /// The line boxes text node `i` was laid out in: the bands of the
    /// floats beside it, or its content width.
    pub fn line_boxes(&self, i: NodeIdx) -> LineBoxes {
        match self.wraps.get(&i) {
            Some(b) => LineBoxes::clone(b),
            None => LineBoxes::plain(self.nodes[i as usize].geo.content_box().w),
        }
    }
}

/// Measures leaf content (text, images) for the layout engine.
pub(crate) trait LeafMeasure {
    /// Size of leaf `node` given known dimensions and available space.
    fn measure(
        &mut self,
        node: &Node,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32>;

    /// The content height of text leaf `node` laid out in `boxes`, beside
    /// floats, and the bottom of its last line that starts beside one.
    fn measure_lines(&mut self, node: &Node, boxes: &LineBoxes) -> Wrapped {
        let width = AvailableSpace::Definite(boxes.width);
        let available = Size {
            width,
            height: AvailableSpace::MaxContent,
        };
        let height = self.measure(node, Size::NONE, available).height;
        Wrapped {
            height,
            wrapped: height,
        }
    }
}

/// Leaves with no intrinsic content (empty boxes, spacers, page breaks).
#[cfg(test)]
pub(crate) struct NoContent;

#[cfg(test)]
impl LeafMeasure for NoContent {
    fn measure(&mut self, _: &Node, _: Size<Option<f32>>, _: Size<AvailableSpace>) -> Size<f32> {
        Size::ZERO
    }
}

/// Dependency firewall around the flex/grid engine: neither it nor
/// [`LeafMeasure`] names a Taffy type.
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

fn edges(r: taffy::geometry::Rect<f32>) -> Edges<f32> {
    Edges {
        top: r.top,
        right: r.right,
        bottom: r.bottom,
        left: r.left,
    }
}

/// How the root of a subtree layout is sized.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RootMode {
    /// The synthetic flow root: a column of fixed width, at least
    /// `min_height` tall (the page content height; 0 for a header or footer).
    Flow { width: f32, min_height: f32 },
    /// A table cell.
    Cell(CellSpace),
    /// A child of a float container, laid out at the width the container
    /// gives it (border box), or at its intrinsic width; with `fit`, a
    /// definite width is only the room it may take, as a leaf measures in.
    /// At a set height for its final layout. Percentage heights are of
    /// `parent_h`.
    Item {
        width: AvailableSpace,
        fit: bool,
        height: Option<f32>,
        parent_h: Option<f32>,
    },
}

/// The space a table gives one of its cells.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CellSpace {
    /// Border-box width, or a content-sizing mode.
    pub width: AvailableSpace,
    /// Final border-box height, when known.
    pub height: Option<f32>,
    /// Collapsed border widths reserved inside the cell.
    pub insets: Edges<f32>,
}

/// Shared state of one layout run: leaf measurement plus table caches.
pub(crate) struct LayoutCtx<'m> {
    pub leaves: &'m mut dyn LeafMeasure,
    /// Table layouts by (table node, width key).
    tables: HashMap<(NodeIdx, SpaceKey), Arc<TableLayout>>,
    /// Cell sizes by (cell node, width key); insets are fixed per cell
    /// within a table.
    cells: HashMap<(NodeIdx, SpaceKey), Size<f32>>,
    /// Closed-form layout of simple cells (`fast.rs`); off only in tests.
    pub fast_cells: bool,
    /// Float container sizes by (container, width key).
    flow_sizes: HashMap<(NodeIdx, SpaceKey), Size<f32>>,
    /// Lay out every column box as a float container: only tests, to check
    /// `flow.rs` against Taffy.
    #[cfg(test)]
    pub flow_all: bool,
}

/// Cache key of a layout width. Exact: bucketing could round below a
/// min-content width and change line breaks.
pub(crate) fn width_key(w: f32) -> u32 {
    if w.is_finite() {
        w.max(0.0).to_bits()
    } else {
        u32::MAX
    }
}

/// Cache key of an available width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SpaceKey {
    Definite(u32),
    MinContent,
    MaxContent,
}

fn space_key(a: AvailableSpace) -> SpaceKey {
    match a {
        AvailableSpace::Definite(w) => SpaceKey::Definite(width_key(w)),
        AvailableSpace::MinContent => SpaceKey::MinContent,
        AvailableSpace::MaxContent => SpaceKey::MaxContent,
    }
}

/// Result of laying out a subtree: geometry relative to the subtree root's
/// top-left corner, for the root and each descendant (tables are leaves).
pub(crate) struct SubLayout {
    pub size: Size<f32>,
    pub geos: Vec<(NodeIdx, Geometry)>,
}

impl<'m> LayoutCtx<'m> {
    pub fn new(leaves: &'m mut dyn LeafMeasure) -> Self {
        Self {
            leaves,
            tables: HashMap::new(),
            cells: HashMap::new(),
            fast_cells: true,
            flow_sizes: HashMap::new(),
            #[cfg(test)]
            flow_all: false,
        }
    }

    /// Whether `flow.rs` lays out the children of node `idx`.
    #[cfg_attr(
        not(test),
        expect(clippy::unused_self, reason = "tests flow every box")
    )]
    fn flows(&self, tree: &Tree<'_>, idx: NodeIdx) -> bool {
        #[cfg(test)]
        if self.flow_all && flow::column_box(tree, idx) {
            return true;
        }
        tree.holds_floats(idx)
    }

    /// Content size of float container `idx` in `available` space.
    fn flow_size(
        &mut self,
        tree: &Tree<'_>,
        idx: NodeIdx,
        available: Size<AvailableSpace>,
        parent_h: Option<f32>,
    ) -> Size<f32> {
        let key = (idx, space_key(available.width));
        if let Some(s) = self.flow_sizes.get(&key) {
            return *s;
        }
        let size = match self.flow_width(tree, idx, available.width, parent_h) {
            Ok(w) => flow::layout(self, tree, idx, w, None, parent_h, false).map(|l| l.size),
            Err(e) => Err(e),
        };
        let size = size.unwrap_or_else(|e| {
            tracing::warn!("float container layout failed: {e}");
            Size::ZERO
        });
        self.flow_sizes.insert(key, size);
        size
    }

    /// The content width float container `idx` lays its children out in.
    fn flow_width(
        &mut self,
        tree: &Tree<'_>,
        idx: NodeIdx,
        width: AvailableSpace,
        parent_h: Option<f32>,
    ) -> Result<f32, InkoxideError> {
        match width {
            AvailableSpace::Definite(w) => Ok(w.max(0.0)),
            mode => flow::intrinsic_width(self, tree, idx, mode, parent_h),
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
        if let (true, RootMode::Cell(space)) = (self.fast_cells, mode) {
            if let Some((child, pad)) = fast::simple_cell(tree, root) {
                if let Some(sub) = fast::cell(tree, root, child, pad, self.leaves, space, collect) {
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
            // Tables and float containers are leaves here.
            let leaf_table = ln.is_table() || self.flows(tree, idx);
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
            let leaf_root = len == 0
                && (matches!(mode, RootMode::Item { .. })
                    || ln.is_table()
                    || self.flows(tree, idx));
            let id = if k == 0 && leaf_root {
                taffy.new_leaf_with_context(root_style(ln, mode), idx)
            } else if k == 0 {
                taffy.new_with_children(root_style(ln, mode), kids)
            } else {
                let mut s = match mode {
                    // The page content box is a page child's containing
                    // block: its percentage heights are of the page.
                    RootMode::Flow { min_height, .. } if k <= list[0].2 && min_height > 0.0 => {
                        style::to_taffy_on_page(ln.style(), min_height)
                    }
                    _ => style::to_taffy(ln.style()),
                };
                // Checkboxes and radio buttons are `size` + 2 pt squares
                // unless sized.
                let button = |n: &Node| {
                    matches!(&n.kind, NodeKind::Field(f)
                        if matches!(f.kind, FieldKind::Checkbox | FieldKind::Radio))
                };
                if ln.node().is_some_and(button) {
                    let parent = tree.nodes[ln.parent as usize].style();
                    style::keep_intrinsic_size(&mut s, ln.style(), parent);
                }
                if len == 0 {
                    taffy.new_leaf_with_context(s, idx)
                } else {
                    taffy.new_with_children(s, kids)
                }
            }
            .map_err(layout_err)?;
            ids[k] = id;
        }

        let width = match mode {
            RootMode::Flow { width, .. } => AvailableSpace::Definite(width),
            RootMode::Cell(space) => space.width,
            RootMode::Item { width, .. } => width,
        };
        // The height the children of a float container resolve their
        // percentages against: the page's, for the flow root.
        let parent_h_of = |i: NodeIdx| -> Option<f32> {
            match mode {
                RootMode::Flow { min_height, .. } if i == root && min_height > 0.0 => {
                    Some(min_height)
                }
                _ => flow::definite_content_height(tree, i),
            }
        };
        let available = TSize {
            width: width.into(),
            height: taffy::AvailableSpace::MaxContent,
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
                        |known, available| {
                            let available = Size {
                                width: available.width.into(),
                                height: available.height.into(),
                            };
                            if self.flows(tree, idx) {
                                return self
                                    .flow_size(tree, idx, available, parent_h_of(idx))
                                    .into();
                            }
                            match nodes[idx as usize].node() {
                                Some(n) if matches!(n.kind, NodeKind::Table(_)) => {
                                    self.table_size(tree, idx, available)
                                }
                                Some(n) => self.leaves.measure(n, known.into(), available),
                                None => Size::ZERO,
                            }
                            .into()
                        },
                    )
                },
            )
            .map_err(layout_err)?;

        let root_layout = taffy.layout(ids[0]).map_err(layout_err)?;
        let size = root_layout.size.into();
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
                        border: edges(l.border),
                        padding: edges(l.padding),
                        margin: edges(l.margin),
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
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        // `available` is already the content box (Taffy subtracts padding
        // and border from a known width).
        match self.table(tree, idx, available.width) {
            Ok(t) => Size {
                width: t.width,
                height: t.height,
            },
            Err(e) => {
                tracing::warn!("table layout failed: {e}");
                Size::ZERO
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
        insets: Edges<f32>,
    ) -> Result<Size<f32>, InkoxideError> {
        let key = (idx, space_key(width));
        if let Some(s) = self.cells.get(&key) {
            return Ok(*s);
        }
        let space = CellSpace {
            width,
            height: None,
            insets,
        };
        let s = self
            .layout_subtree(tree, idx, RootMode::Cell(space), false)?
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
        RootMode::Cell(CellSpace {
            width,
            height,
            insets,
        }) => {
            let mut s = style::to_taffy(ln.style());
            // Collapsed borders are reserved as Taffy borders (the table
            // paints them).
            let [t, r, b, l] = insets.to_array().map(LengthPercentage::length);
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
            s.size.height = Dimension::auto();
            s.min_size.height = style::cell_min_height(ln.style());
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
        RootMode::Item {
            width,
            fit,
            height,
            parent_h,
        } => {
            let mut s = match parent_h {
                Some(h) => style::to_taffy_on_page(ln.style(), h),
                None => style::to_taffy(ln.style()),
            };
            // The container places it: no margins or offsets of its own.
            s.margin = taffy::geometry::Rect::zero();
            s.inset = taffy::geometry::Rect::auto();
            s.position = taffy::style::Position::Relative;
            if let (AvailableSpace::Definite(w), false) = (width, fit) {
                s.size.width = Dimension::length(w);
                s.min_size.width = LengthPercentageAuto::auto();
                s.max_size.width = LengthPercentageAuto::auto();
            }
            if let Some(h) = height {
                s.size.height = Dimension::length(h);
                s.min_size.height = LengthPercentageAuto::length(h);
                s.max_size.height = LengthPercentageAuto::length(h);
            }
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
fn compute_tree(
    tree: &mut Tree<'_>,
    width: f32,
    min_height: f32,
    measure: &mut dyn LeafMeasure,
    fast_cells: bool,
) -> Result<(), InkoxideError> {
    let mut ctx = LayoutCtx::new(measure);
    ctx.fast_cells = fast_cells;
    lay_out_tree(&mut ctx, tree, width, min_height)
}

/// [`compute_tree`] with every column box laid out by `flow.rs`, for
/// tests that check it against Taffy.
#[cfg(test)]
fn compute_tree_flowing(
    tree: &mut Tree<'_>,
    width: f32,
    min_height: f32,
    measure: &mut dyn LeafMeasure,
) -> Result<(), InkoxideError> {
    let mut ctx = LayoutCtx::new(measure);
    ctx.flow_all = true;
    lay_out_tree(&mut ctx, tree, width, min_height)
}

/// Lays out `tree` with `ctx`, writing every node's geometry.
fn lay_out_tree(
    ctx: &mut LayoutCtx<'_>,
    tree: &mut Tree<'_>,
    width: f32,
    min_height: f32,
) -> Result<(), InkoxideError> {
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
            let Rect {
                x: cx,
                y: cy,
                w: cw,
                ..
            } = tree.nodes[i].geo.content_box();
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
                let space = CellSpace {
                    width: AvailableSpace::Definite(cell.w),
                    height: Some(cell.h),
                    insets: cell.insets,
                };
                let sub = ctx.layout_subtree(tree, cell.idx, RootMode::Cell(space), true)?;
                let (ox, oy) = (cx + cell.x, cy + cell.y);
                for (n, g) in sub.geos {
                    tree.nodes[n as usize].geo = g.offset(ox, oy);
                }
            }
            tree.tables.insert(idx, layout);
        }
        if ctx.flows(tree, i as NodeIdx) {
            // A float container: its children, at its final size.
            let idx = i as NodeIdx;
            let cb = tree.nodes[i].geo.content_box();
            let parent_h = if i == 0 && min_height > 0.0 {
                Some(min_height)
            } else {
                flow::definite_content_height(tree, idx)
            };
            let fl = flow::layout(ctx, tree, idx, cb.w, Some(cb.h), parent_h, true)?;
            for (n, g) in fl.geos {
                tree.nodes[n as usize].geo = g.offset(cb.x, cb.y);
            }
            tree.wraps.extend(fl.wraps);
            for (f, end) in fl.groups {
                tree.float_groups.insert(f, cb.y + end);
            }
        }
        i += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
