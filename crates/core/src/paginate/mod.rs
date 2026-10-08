//! Pagination: slices a flow laid out in one unbounded column into pages.
//!
//! One layout pass, no re-layout: pagination walks the flow in document
//! order and accumulates a vertical **shift** whenever content has to move
//! to the next page. Every node records the shift at its top and at its
//! bottom (they differ only for nodes split across pages); split
//! paragraphs record a shift per line. A node's position in *paged space*
//! is its layout position plus its shift; page `k` shows paged space
//! `[k·H, (k+1)·H)`.
//!
//! Rules (§6.4):
//! - `break.before/after: page` and `pageBreak` nodes force a new page
//!   (never an empty one).
//! - A node whose first unbreakable piece (its *head*) does not fit in the
//!   rest of the page moves whole to the next page, so a box never leaves
//!   just its top border/padding behind. `minPresenceAhead` raises the head.
//! - Atomic nodes (`inside: avoid`, row/grid/reversed boxes, images,
//!   spacers, tables until M4, positioned, rotated or fixed nodes) move
//!   whole; taller than a page, they are sliced (with a warning).
//! - Column boxes split between children; text splits between lines.

use crate::layout::{NodeIdx, Tree};
use crate::schema::{
    BreakMode, Display, FlexDirection, FlexWrap, InsideMode, Node, NodeKind, Position,
};
use crate::text::{LineLayout, TextCache, TextShaper};

/// Float tolerance in points. Paged coordinates of long documents reach
/// ~10⁶ pt, where one f32 step is ~0.06 pt; 0.1 pt is still invisible.
pub(crate) const EPS: f32 = 0.1;

/// Result of paginating one flow.
#[derive(Debug, Clone)]
pub(crate) struct Placement {
    /// Page content height.
    pub page_h: f32,
    pub pages: u32,
    pub shift_top: Vec<f32>,
    pub shift_bottom: Vec<f32>,
    /// Per-line shifts of split paragraphs (empty: use `shift_top`).
    pub line_shifts: Vec<Vec<f32>>,
    /// First and last page each node or any descendant touches.
    pub page_range: Vec<(u32, u32)>,
    /// Nodes with `break.fixed`: painted on every page.
    pub fixed: Vec<NodeIdx>,
}

impl Placement {
    /// Paged top and height of node `i`.
    pub fn paged_rect(&self, tree: &Tree<'_>, i: NodeIdx) -> (f32, f32) {
        let g = &tree.nodes[i as usize].geo;
        let st = self.shift_top[i as usize];
        let sb = self.shift_bottom[i as usize];
        (g.y + st, g.h + sb - st)
    }

    pub fn on_page(&self, i: NodeIdx, page: u32) -> bool {
        let (a, b) = self.page_range[i as usize];
        a <= page && page <= b
    }
}

/// Supplies line layouts of text nodes at their laid-out width.
pub(crate) trait LineSource {
    fn lines(&mut self, node: &Node, width: f32) -> Option<std::sync::Arc<LineLayout>>;
}

impl<S: TextShaper> LineSource for TextCache<'_, S> {
    fn lines(&mut self, node: &Node, width: f32) -> Option<std::sync::Arc<LineLayout>> {
        self.layout(node, width)
    }
}

struct Paginator<'t, 'a, 'l> {
    tree: &'t Tree<'a>,
    lines: &'l mut dyn LineSource,
    h: f32,
    shift: f32,
    out: Placement,
}

pub(crate) fn paginate(tree: &Tree<'_>, page_h: f32, lines: &mut dyn LineSource) -> Placement {
    let n = tree.nodes.len();
    let mut p = Paginator {
        tree,
        lines,
        h: page_h,
        shift: 0.0,
        out: Placement {
            page_h,
            pages: 1,
            shift_top: vec![0.0; n],
            shift_bottom: vec![0.0; n],
            line_shifts: vec![Vec::new(); n],
            page_range: vec![(0, 0); n],
            fixed: Vec::new(),
        },
    };
    // The synthetic root is a splittable column.
    p.out.shift_top[0] = 0.0;
    for c in tree.children(0) {
        p.place(c);
    }
    p.out.shift_bottom[0] = p.shift;

    // Page ranges, bottom-up (children have larger indices than parents).
    let mut last_page = 0u32;
    for i in (0..n).rev() {
        let (top, h) = p.out.paged_rect(tree, i as NodeIdx);
        let first = p.page_of(top);
        let last = p.page_of_bottom(top + h);
        let mut range = (first, last.max(first));
        let kids = tree.nodes[i].children.clone();
        for c in kids {
            let r = p.out.page_range[c as usize];
            range = (range.0.min(r.0), range.1.max(r.1));
        }
        p.out.page_range[i] = range;
        if i != 0 {
            last_page = last_page.max(range.1);
        }
    }
    // The root's own extent (min-height = one page) counts too.
    let (rt, rh) = p.out.paged_rect(tree, 0);
    last_page = last_page.max(p.page_of_bottom(rt + rh));
    p.out.pages = last_page + 1;
    p.out.page_range[0] = (0, last_page);

    // Fixed nodes (outermost only), in document order.
    let mut fixed_below = vec![false; n];
    for i in 1..n {
        let parent = tree.nodes[i].parent as usize;
        let is_fixed = tree.nodes[i].node.is_some_and(|nd| nd.brk.fixed);
        if is_fixed && !fixed_below[parent] {
            p.out.fixed.push(i as NodeIdx);
        }
        fixed_below[i] = fixed_below[parent] || is_fixed;
    }
    p.out.fixed.sort_unstable();
    p.out
}

impl Paginator<'_, '_, '_> {
    fn page_of(&self, y: f32) -> u32 {
        ((y + EPS) / self.h).floor().max(0.0) as u32
    }

    /// Page holding the content that ends at paged `y` (a bottom exactly on
    /// a page boundary belongs to the page above).
    fn page_of_bottom(&self, y: f32) -> u32 {
        ((y - EPS) / self.h).floor().max(0.0) as u32
    }

    fn page_end(&self, y: f32) -> f32 {
        (self.page_of(y) + 1) as f32 * self.h
    }

    fn at_page_top(&self, y: f32) -> bool {
        let r = y - self.page_of(y) as f32 * self.h;
        r.abs() < EPS
    }

    /// Moves everything from paged `y` on to the next page.
    fn push(&mut self, y: f32) {
        if !self.at_page_top(y) {
            self.shift += self.page_end(y) - y;
        }
    }

    fn set_subtree(&mut self, i: NodeIdx) {
        let s = self.shift;
        let mut stack = vec![i];
        while let Some(j) = stack.pop() {
            self.out.shift_top[j as usize] = s;
            self.out.shift_bottom[j as usize] = s;
            stack.extend(self.tree.children(j));
        }
    }

    /// Whether a node must move as one piece.
    fn atomic(node: &Node) -> bool {
        let s = &node.style;
        if node.brk.inside == InsideMode::Avoid
            || node.brk.fixed
            || s.position == Some(Position::Absolute)
            || s.rotate.is_some_and(|r| r % 360.0 != 0.0)
            || s.transform.as_ref().is_some_and(|t| !t.is_empty())
        {
            return true;
        }
        match &node.kind {
            NodeKind::Text { .. } | NodeKind::Table(_) => false,
            NodeKind::Box { children } => {
                children.is_empty()
                    || s.display == Some(Display::Grid)
                    || matches!(
                        s.flex_direction,
                        Some(
                            FlexDirection::Row
                                | FlexDirection::RowReverse
                                | FlexDirection::ColumnReverse
                        )
                    )
                    || matches!(s.flex_wrap, Some(FlexWrap::Wrap | FlexWrap::WrapReverse))
            }
            _ => true,
        }
    }

    /// Height from the node's top to the end of its first unbreakable
    /// piece.
    fn head(&mut self, i: NodeIdx) -> f32 {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node else { return 0.0 };
        let g = ln.geo;
        if Self::atomic(node) {
            return g.h;
        }
        match &node.kind {
            NodeKind::Text { .. } => {
                let (_, cy, cw, _) = g.content_box();
                match self.lines.lines(node, cw) {
                    Some(l) if !l.lines.is_empty() => {
                        let first = &l.lines[0];
                        (cy - g.y) + first.top + first.height
                    }
                    _ => g.h,
                }
            }
            NodeKind::Table(_) => {
                // Header rows plus the first body row group.
                let Some(t) = self.tree.tables.get(&i).cloned() else {
                    return g.h;
                };
                let (_, cy, _, _) = g.content_box();
                let first_body = t.groups.iter().find(|r| r.start >= t.header_rows);
                let end = first_body.map_or(t.row_y.len(), |r| r.end);
                let bottom = if end == 0 {
                    0.0
                } else {
                    t.row_y[end - 1] + t.row_h[end - 1]
                };
                (cy - g.y) + bottom
            }
            NodeKind::Box { .. } => {
                let first = self.tree.children(i).find(|&c| self.in_flow(c));
                match first {
                    Some(c) => {
                        let cy = self.tree.nodes[c as usize].geo.y;
                        (cy - g.y) + self.head(c)
                    }
                    None => g.h,
                }
            }
            _ => g.h,
        }
    }

    /// Children that take part in pagination (not absolutely positioned,
    /// not hidden).
    fn in_flow(&self, c: NodeIdx) -> bool {
        self.tree.nodes[c as usize].node.is_some_and(|n| {
            n.style.position != Some(Position::Absolute) && n.style.display != Some(Display::None)
        })
    }

    fn place(&mut self, i: NodeIdx) {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node else { return };
        let g = ln.geo;

        if node.style.display == Some(Display::None)
            || node.style.position == Some(Position::Absolute)
        {
            // Positioned boxes travel with the shift at their position.
            self.set_subtree(i);
            return;
        }
        if matches!(node.kind, NodeKind::PageBreak) {
            self.push(g.y + self.shift);
            self.set_subtree(i);
            return;
        }
        if node.brk.before == BreakMode::Page {
            self.push(g.y + self.shift);
        }

        let top = g.y + self.shift;
        let need = self.head(i).max(node.brk.min_presence_ahead.unwrap_or(0.0));
        if top + need > self.page_end(top) + EPS {
            self.push(top);
        }
        let top = g.y + self.shift;
        let fits = top + g.h <= self.page_end(top) + EPS;
        let atomic = Self::atomic(node);

        if fits || atomic {
            if !fits {
                tracing::warn!(
                    node = node.id.as_deref().unwrap_or(""),
                    height = g.h,
                    page = self.h,
                    "unsplittable content is taller than a page and is sliced"
                );
            }
            self.set_subtree(i);
        } else {
            self.out.shift_top[i as usize] = self.shift;
            match &node.kind {
                NodeKind::Text { .. } => self.split_text(i, node),
                NodeKind::Table(_) => self.split_table(i),
                NodeKind::Box { .. } => {
                    let kids: Vec<NodeIdx> = self.tree.children(i).collect();
                    let box_shift = self.shift;
                    for c in kids {
                        if self.in_flow(c) {
                            self.place(c);
                        } else {
                            // Out-of-flow children stay with the box top.
                            let s = self.shift;
                            self.shift = box_shift;
                            self.set_subtree(c);
                            self.shift = s;
                        }
                    }
                }
                _ => {}
            }
            self.out.shift_bottom[i as usize] = self.shift;
        }

        if node.brk.after == BreakMode::Page {
            self.push(g.y + g.h + self.shift);
        }
    }

    /// Places a table row group by row group. A group that does not fit
    /// moves to the next page, leaving room for the repeated header rows.
    fn split_table(&mut self, i: NodeIdx) {
        let Some(t) = self.tree.tables.get(&i).cloned() else {
            self.set_subtree(i);
            return;
        };
        let rows: Vec<NodeIdx> = self.tree.children(i).collect();
        let header_h = t.header_height();
        // Paged y where body rows start on the current continuation page;
        // a group placed there is never pushed again (it is sliced).
        let mut fresh_start = f32::NEG_INFINITY;
        let table_top = self.tree.nodes[i as usize].geo.y + self.shift;
        for group in &t.groups {
            let first = rows[group.start];
            let g_top = self.tree.nodes[first as usize].geo.y;
            let g_h: f32 = t.row_h[group.clone()].iter().sum();
            let top = g_top + self.shift;
            let is_header = group.end <= t.header_rows;
            if !is_header && (top - fresh_start).abs() > EPS {
                let overflows = top + g_h > self.page_end(top) + EPS && !self.at_page_top(top);
                // A group that lands exactly at the top of a later page
                // (without being pushed) also needs room for the header.
                let at_new_page =
                    self.at_page_top(top) && self.page_of(top) > self.page_of(table_top);
                if overflows || at_new_page {
                    let to_next = if overflows {
                        self.page_end(top) - top
                    } else {
                        0.0
                    };
                    self.shift += to_next + header_h;
                    fresh_start = g_top + self.shift;
                    if g_h + header_h > self.h + EPS {
                        tracing::warn!(height = g_h, "table row taller than a page is sliced");
                    }
                }
            }
            for r in group.clone() {
                self.set_subtree(rows[r]);
            }
        }
    }

    fn split_text(&mut self, i: NodeIdx, node: &Node) {
        let g = self.tree.nodes[i as usize].geo;
        let (_, cy, cw, _) = g.content_box();
        let Some(layout) = self.lines.lines(node, cw) else {
            return;
        };
        let mut shifts = Vec::with_capacity(layout.lines.len());
        for line in &layout.lines {
            let top = cy + line.top + self.shift;
            if top + line.height > self.page_end(top) + EPS && !self.at_page_top(top) {
                self.push(top);
            }
            shifts.push(self.shift);
        }
        self.out.line_shifts[i as usize] = shifts;
    }
}

#[cfg(test)]
mod tests;
