//! Taffy's flexbox and grid algorithms over one subtree, with every size
//! they ask a node for remembered.
//!
//! Taffy keeps the sizes of a node in a few slots, picked by the kind of
//! question (which sides are known, what room there is): two questions of
//! one kind with different numbers take turns in a slot. A box whose
//! parent asks it two such questions asks its own child two each time,
//! and so on down: nested boxes took time exponential in their depth,
//! though each was only ever asked a handful of different questions. The
//! answers here are kept by the whole question, so each is worked out
//! once.
//!
//! A layout is a question too (a grid lays its items out to find their
//! baselines, then again in their cells), but its answer comes with the
//! places of the node's descendants. An answer that is reused leaves
//! those as the last layout worked out put them; the node is laid out
//! once more at the end if that was another one.

use std::collections::HashMap;

use taffy::{
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_hidden_layout,
    compute_root_layout, AvailableSpace, Cache, CacheTree, CoreStyle, Display, Layout,
    LayoutFlexboxContainer, LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree,
    NodeId, RequestedAxis, RunMode, Size, SizingMode, Style, TraversePartialTree,
};

use super::NodeIdx;

/// Answers worked out for one subtree before it is refused, besides
/// [`WORK_PER_NODE`] for each of its nodes: a backstop, for questions that
/// do differ at every level (boxes nested with percentages get a new
/// width each, and are asked more of them the deeper they are). Ordinary
/// trees take about ten per node, at any depth.
const BASE_WORK: u64 = 20_000;

/// See [`BASE_WORK`].
const WORK_PER_NODE: u64 = 256;

/// A node of the subtree: its children are the nodes `children`.
pub(super) struct SolveNode {
    pub(super) style: Style,
    pub(super) children: std::ops::Range<usize>,
    /// The tree node a childless node is measured as.
    pub(super) leaf: Option<NodeIdx>,
}

struct Slot {
    node: SolveNode,
    cache: Cache,
    layout: Layout,
    /// The layout its descendants are placed by: the last worked out.
    placed: Option<Asked>,
    /// The layout it was last asked for, when the answer was reused.
    wanted: Option<LayoutInput>,
}

/// A question about a node: every part of a [`LayoutInput`] that computes
/// a size or lays the node out.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Asked {
    node: usize,
    layout: bool,
    sizing: bool,
    axis: u8,
    known: [u64; 2],
    definite: [bool; 2],
    parent: [u64; 2],
    available: [u64; 2],
    collapsible: [bool; 2],
}

impl Asked {
    fn new(node: usize, i: &LayoutInput) -> Self {
        // Past any `f32`'s bits.
        let length = |v: Option<f32>| v.map_or(1 << 32, |v| u64::from(v.to_bits()));
        let space = |a: AvailableSpace| match a {
            AvailableSpace::Definite(v) => u64::from(v.to_bits()),
            AvailableSpace::MinContent => 1 << 32,
            AvailableSpace::MaxContent => 2 << 32,
        };
        Self {
            node,
            layout: i.run_mode == RunMode::PerformLayout,
            sizing: i.sizing_mode == SizingMode::ContentSize,
            axis: match i.axis {
                RequestedAxis::Horizontal => 0,
                RequestedAxis::Vertical => 1,
                RequestedAxis::Both => 2,
            },
            known: [
                length(i.known_dimensions.width),
                length(i.known_dimensions.height),
            ],
            definite: [
                i.known_dimensions_are_definite.width,
                i.known_dimensions_are_definite.height,
            ],
            parent: [length(i.parent_size.width), length(i.parent_size.height)],
            available: [
                space(i.available_space.width),
                space(i.available_space.height),
            ],
            collapsible: [
                i.vertical_margins_are_collapsible.start,
                i.vertical_margins_are_collapsible.end,
            ],
        }
    }
}

/// The subtree being laid out; `measure` lays out a childless node.
struct Solve<F> {
    slots: Vec<Slot>,
    measure: F,
    /// The answers worked out.
    answers: HashMap<Asked, LayoutOutput>,
    /// Answers worked out, and the most there may be.
    work: u64,
    budget: u64,
}

/// Lays `nodes` out, the first of them the root, in `available`: the
/// layout of each, in order, relative to its parent. `None` for a subtree
/// that takes more work than any real one.
pub(super) fn solve<F>(
    nodes: Vec<SolveNode>,
    available: Size<AvailableSpace>,
    measure: F,
) -> Option<Vec<Layout>>
where
    F: FnMut(LayoutInput, Option<NodeIdx>, &Style) -> LayoutOutput,
{
    let budget = BASE_WORK.saturating_add(WORK_PER_NODE.saturating_mul(nodes.len() as u64));
    let slots = nodes.into_iter().map(|node| Slot {
        node,
        cache: Cache::new(),
        layout: Layout::new(),
        placed: None,
        wanted: None,
    });
    let mut solve = Solve {
        slots: slots.collect(),
        measure,
        answers: HashMap::new(),
        work: 0,
        budget,
    };
    compute_root_layout(&mut solve, NodeId::from(0usize), available);
    solve.place();
    (solve.work <= solve.budget).then(|| solve.slots.into_iter().map(|s| s.layout).collect())
}

impl<F> Solve<F>
where
    F: FnMut(LayoutInput, Option<NodeIdx>, &Style) -> LayoutOutput,
{
    /// Lays node `id` out with the algorithm of its `display`, as
    /// `TaffyTree` does.
    fn compute(&mut self, id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.work += 1;
        let slot = &self.slots[usize::from(id)];
        match (slot.node.style.display, slot.node.children.is_empty()) {
            (Display::None, _) => compute_hidden_layout(self, id),
            (Display::Flex, false) => compute_flexbox_layout(self, id, inputs),
            (Display::Grid, false) => compute_grid_layout(self, id, inputs),
            (_, true) => (self.measure)(inputs, slot.node.leaf, &slot.node.style),
        }
    }

    /// Lays out again each node whose descendants another layout placed
    /// than the one it was last asked for: parents first, since that
    /// asks the children for theirs.
    fn place(&mut self) {
        for k in 0..self.slots.len() {
            let Some(inputs) = self.slots[k].wanted.take() else {
                continue;
            };
            let asked = Asked::new(k, &inputs);
            if self.slots[k].placed.as_ref() != Some(&asked) && self.work <= self.budget {
                self.compute(NodeId::from(k), inputs);
                self.slots[k].placed = Some(asked);
            }
        }
    }
}

impl<F> TraversePartialTree for Solve<F> {
    type ChildIter<'a>
        = std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeId>
    where
        Self: 'a;

    fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
        let children = self.slots[usize::from(parent)].node.children.clone();
        children.map(NodeId::from as fn(usize) -> NodeId)
    }

    fn child_count(&self, parent: NodeId) -> usize {
        self.slots[usize::from(parent)].node.children.len()
    }

    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        NodeId::from(self.slots[usize::from(parent)].node.children.start + index)
    }
}

impl<F> LayoutPartialTree for Solve<F>
where
    F: FnMut(LayoutInput, Option<NodeIdx>, &Style) -> LayoutOutput,
{
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type CustomIdent = <Style as CoreStyle>::CustomIdent;

    fn get_core_container_style(&self, id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.slots[usize::from(id)].node.style
    }

    fn set_unrounded_layout(&mut self, id: NodeId, layout: &Layout) {
        self.slots[usize::from(id)].layout = *layout;
    }

    fn compute_child_layout(&mut self, id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        // Past the budget nothing is worked out any more: the algorithms
        // above return, each after one pass over its children.
        if self.work > self.budget {
            return LayoutOutput::HIDDEN;
        }
        let k = usize::from(id);
        let layout = inputs.run_mode == RunMode::PerformLayout;
        // Under a hidden ancestor, whatever its own `display`.
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, id);
        }
        let asked = Asked::new(k, &inputs);
        if let Some(&answer) = self.answers.get(&asked) {
            if layout {
                self.slots[k].wanted = Some(inputs);
            }
            return answer;
        }
        let before = self.work;
        let out = compute_cached_layout(self, id, inputs, Self::compute);
        if layout {
            // Worked out now, or what Taffy kept from the last time.
            self.slots[k].placed = Some(asked.clone());
            self.slots[k].wanted = None;
        }
        if self.work != before && self.work <= self.budget {
            self.answers.insert(asked, out);
        }
        out
    }
}

impl<F> CacheTree for Solve<F> {
    fn cache_get(&mut self, id: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.slots[usize::from(id)].cache.get(input)
    }

    fn cache_store(&mut self, id: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.slots[usize::from(id)].cache.store(input, output);
    }

    fn cache_clear(&mut self, id: NodeId) {
        self.slots[usize::from(id)].cache.clear();
    }
}

impl<F> LayoutFlexboxContainer for Solve<F>
where
    F: FnMut(LayoutInput, Option<NodeIdx>, &Style) -> LayoutOutput,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.slots[usize::from(id)].node.style
    }

    fn get_flexbox_child_style(&self, child: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.slots[usize::from(child)].node.style
    }
}

impl<F> LayoutGridContainer for Solve<F>
where
    F: FnMut(LayoutInput, Option<NodeIdx>, &Style) -> LayoutOutput,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_grid_container_style(&self, id: NodeId) -> Self::GridContainerStyle<'_> {
        &self.slots[usize::from(id)].node.style
    }

    fn get_grid_child_style(&self, child: NodeId) -> Self::GridItemStyle<'_> {
        &self.slots[usize::from(child)].node.style
    }
}
