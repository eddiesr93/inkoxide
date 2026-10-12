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
//! Rules:
//! - `break.before/after: page` and `pageBreak` nodes force a new page
//!   when in-flow content follows them, at any depth (never an empty
//!   page). A break before a box's first child moves the box with it.
//! - A node whose first unbreakable piece (its *head*) does not fit in the
//!   rest of the page moves whole to the next page, so a box never leaves
//!   just its top border/padding behind. `minPresenceAhead` raises the head.
//! - `inside: avoid` nodes and row, grid, wrapping or reversed boxes move
//!   whole when they fit on a page. Taller than a page, they split like
//!   other content: a row or grid box band by band (children whose extents
//!   overlap), each child paginating from the band top like a table cell.
//!   A forced break inside an `inside: avoid` box wins (as in CSS); the
//!   part before it moves whole.
//! - Atomic nodes (images, spacers, empty boxes, positioned, rotated or
//!   fixed nodes) move whole; taller than a page, they are sliced (with a
//!   warning).
//! - Column boxes split between children; text splits between lines.

use crate::layout::{NodeIdx, Tree};
use crate::paint::Rect;
use crate::schema::{
    BreakMode, Display, FlexDirection, FlexWrap, InsideMode, Node, NodeKind, Position,
};
use crate::text::{LineLayout, TextCache, TextShaper};

/// Float tolerance in paged space, points. Paged coordinates of long
/// documents reach ~10⁶ pt, where one f32 step is ~0.06 pt; 0.1 pt is
/// still invisible.
pub(crate) const PAGED_EPS: f32 = 0.1;

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
    /// Nodes with `break.fixed` (outermost only): painted on every page, at
    /// their place on their first page, in paint order. Overlays are not in
    /// the flow tree.
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

    /// First page node `i` or any descendant touches.
    pub fn first_page(&self, i: NodeIdx) -> u32 {
        self.page_range[i as usize].0
    }

    /// Shift of node `i` into paged space (at its top).
    pub fn shift(&self, i: NodeIdx) -> f32 {
        self.shift_top[i as usize]
    }

    /// Growth of node `i` from being split across pages.
    pub fn growth(&self, i: NodeIdx) -> f32 {
        self.shift_bottom[i as usize] - self.shift_top[i as usize]
    }

    /// Whether node `i` is split across pages.
    pub fn is_split(&self, i: NodeIdx) -> bool {
        self.shift_bottom[i as usize] > self.shift_top[i as usize]
    }

    /// Per-line shifts of node `i`, a split paragraph (empty: use
    /// [`Placement::shift`]).
    pub fn line_shifts(&self, i: NodeIdx) -> &[f32] {
        &self.line_shifts[i as usize]
    }

    /// Paged y of the top of page `page`.
    pub fn page_top(&self, page: u32) -> f32 {
        page as f32 * self.page_h
    }

    /// The paged y range shown on page `page`.
    pub fn window(&self, page: u32) -> PageWindow {
        PageWindow {
            top: self.page_top(page),
            bottom: self.page_top(page + 1),
        }
    }

    /// The first page of node `i`, and where flow y `y` of the node lands
    /// on it, from the page's top.
    pub fn locate(&self, i: NodeIdx, y: f32) -> (u32, f32) {
        let page = self.first_page(i);
        (page, y + self.shift(i) - self.page_top(page))
    }
}

/// The paged y range `[top, bottom)` one page shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PageWindow {
    pub top: f32,
    pub bottom: f32,
}

impl PageWindow {
    /// Whether content from paged `top`, `height` high, misses the page:
    /// it starts below it or ends above it, give or take a hair.
    pub fn hides(self, top: f32, height: f32) -> bool {
        /// Overlap (points) too thin to count as being on the page.
        const HAIR: f32 = 0.01;
        top >= self.bottom - HAIR || top + height <= self.top + HAIR
    }

    /// The part of `top..bottom` (page coordinates) shown on the page, for
    /// a page whose paged y 0 is at page y `dy`.
    pub fn clip(self, top: f32, bottom: f32, dy: f32) -> (f32, f32) {
        (top.max(self.top + dy), bottom.min(self.bottom + dy))
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
    /// Space kept free at the top of each new page (repeated table header
    /// rows while a tall row splits).
    reserve: f32,
    /// Nodes that hold a forced break below them, in flowing content.
    forced: Vec<bool>,
    /// Nodes whose first in-flow descendant is (or has) a forced break.
    leads: Vec<bool>,
    /// Paged y of a forced break taken by the next in-flow node, and the
    /// `pageBreak` nodes that go to the top of that node's page.
    pending: Option<f32>,
    pending_nodes: Vec<NodeIdx>,
    /// The parent took the break before the next node placed (its first
    /// in-flow child).
    lead_taken: bool,
    out: Placement,
}

pub(crate) fn paginate(tree: &Tree<'_>, page_h: f32, lines: &mut dyn LineSource) -> Placement {
    let n = tree.nodes.len();
    let (forced, leads) = forced_breaks(tree);
    let mut p = Paginator {
        tree,
        lines,
        h: page_h,
        shift: 0.0,
        reserve: 0.0,
        forced,
        leads,
        pending: None,
        pending_nodes: Vec::new(),
        lead_taken: false,
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
    // The synthetic root is a splittable column. A forced break still
    // pending at its end has nothing to move, so it adds no page.
    p.place_children(0, 0.0);
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

    // Fixed nodes (outermost only), in the order the flow would paint
    // them: depth first, siblings by `zIndex` then document order (the
    // arena is breadth first).
    let mut stack = vec![0];
    while let Some(i) = stack.pop() {
        if i != 0 && tree.nodes[i as usize].node.is_some_and(|nd| nd.brk.fixed) {
            p.out.fixed.push(i);
            continue;
        }
        let mut kids: Vec<NodeIdx> = tree.children(i).collect();
        kids.sort_by_key(|&c| {
            tree.nodes[c as usize]
                .node
                .and_then(|nd| nd.style.z_index)
                .unwrap_or(0)
        });
        stack.extend(kids.into_iter().rev());
    }
    p.out
}

/// Whether a node lays its children out side by side (row, grid, wrapping)
/// or in reverse: as a box taller than a page, it splits band by band.
pub(crate) fn banded(node: &Node) -> bool {
    let s = &node.style;
    s.display == Some(Display::Grid)
        || matches!(
            s.flex_direction,
            Some(FlexDirection::Row | FlexDirection::RowReverse | FlexDirection::ColumnReverse)
        )
        || matches!(s.flex_wrap, Some(FlexWrap::Wrap | FlexWrap::WrapReverse))
}

/// Per node: whether a forced break (`pageBreak`, `break.before/after:
/// page`) sits below it in flowing content, and whether its first in-flow
/// child is or starts with one.
fn forced_breaks(tree: &Tree<'_>) -> (Vec<bool>, Vec<bool>) {
    let n = tree.nodes.len();
    let (mut forced, mut leads) = (vec![false; n], vec![false; n]);
    // Children have larger indices than parents.
    for i in (0..n).rev() {
        let mut first = true;
        for c in tree.children(i as NodeIdx) {
            let Some(node) = tree.nodes[c as usize].node else {
                continue;
            };
            if node.style.position == Some(Position::Absolute)
                || node.style.display == Some(Display::None)
            {
                continue;
            }
            let own = matches!(node.kind, NodeKind::PageBreak)
                || node.brk.before == BreakMode::Page
                || node.brk.after == BreakMode::Page;
            forced[i] |= own || forced[c as usize];
            if first {
                leads[i] = matches!(node.kind, NodeKind::PageBreak)
                    || node.brk.before == BreakMode::Page
                    || leads[c as usize];
                first = false;
            }
        }
    }
    (forced, leads)
}

impl Paginator<'_, '_, '_> {
    fn page_of(&self, y: f32) -> u32 {
        ((y + PAGED_EPS) / self.h).floor().max(0.0) as u32
    }

    /// Page holding the content that ends at paged `y` (a bottom exactly on
    /// a page boundary belongs to the page above).
    fn page_of_bottom(&self, y: f32) -> u32 {
        ((y - PAGED_EPS) / self.h).floor().max(0.0) as u32
    }

    fn page_end(&self, y: f32) -> f32 {
        (self.page_of(y) + 1) as f32 * self.h
    }

    fn at_page_top(&self, y: f32) -> bool {
        let r = y - self.page_of(y) as f32 * self.h;
        r.abs() < PAGED_EPS
    }

    /// Moves everything from paged `y` on to the next page.
    fn push(&mut self, y: f32) {
        if !self.at_page_top(y) {
            self.shift += self.page_end(y) - y + self.reserve;
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

    /// Whether node `i` must move as one piece.
    fn atomic(&self, i: NodeIdx, node: &Node) -> bool {
        let s = &node.style;
        if node.brk.fixed
            || s.position == Some(Position::Absolute)
            || s.rotate.is_some_and(|r| r % 360.0 != 0.0)
            || s.transform.as_ref().is_some_and(|t| !t.is_empty())
        {
            return true;
        }
        // Kept together only while that is possible.
        let tall = self.tree.nodes[i as usize].geo.h > self.h + PAGED_EPS;
        if node.brk.inside == InsideMode::Avoid && !tall && !self.forced[i as usize] {
            return true;
        }
        match &node.kind {
            NodeKind::Text { .. } | NodeKind::Table(_) => false,
            NodeKind::Box { children } => children.is_empty() || (banded(node) && !tall),
            _ => true,
        }
    }

    /// The in-flow children of `i` from top to bottom, grouped into bands:
    /// children whose laid-out extents overlap (a flex line, a grid row and
    /// the rows its items span).
    fn bands(&self, i: NodeIdx) -> Vec<Vec<NodeIdx>> {
        let geo = |c: NodeIdx| self.tree.nodes[c as usize].geo;
        let mut kids: Vec<NodeIdx> = self.tree.children(i).filter(|&c| self.in_flow(c)).collect();
        kids.sort_by(|&a, &b| geo(a).y.total_cmp(&geo(b).y));
        let mut bands: Vec<Vec<NodeIdx>> = Vec::new();
        let mut bottom = f32::NEG_INFINITY;
        for c in kids {
            let g = geo(c);
            match bands.last_mut() {
                Some(band) if g.y < bottom - PAGED_EPS => {
                    band.push(c);
                    bottom = bottom.max(g.y + g.h);
                }
                _ => {
                    bands.push(vec![c]);
                    bottom = g.y + g.h;
                }
            }
        }
        bands
    }

    /// Height from the node's top to the end of its first unbreakable
    /// piece.
    fn head(&mut self, i: NodeIdx) -> f32 {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node else { return 0.0 };
        let g = ln.geo;
        if self.atomic(i, node) {
            return g.h;
        }
        match &node.kind {
            NodeKind::Text { .. } => {
                let Rect { y: cy, w: cw, .. } = g.content_box();
                let (orphans, widows) = Self::orphans_widows(node);
                match self.lines.lines(node, cw) {
                    // Too short to leave `orphans` and carry `widows` lines:
                    // it moves whole.
                    Some(l) if l.lines.len() >= orphans + widows => {
                        let last = &l.lines[orphans - 1];
                        (cy - g.y) + last.top + last.height
                    }
                    _ => g.h,
                }
            }
            NodeKind::Table(_) => {
                // Header rows plus the first body row group.
                let Some(t) = self.tree.tables.get(&i).cloned() else {
                    return g.h;
                };
                let Rect { y: cy, .. } = g.content_box();
                let first_body = t.groups.iter().find(|r| r.start >= t.header_rows);
                let end = first_body.map_or(t.row_y.len(), |r| r.end);
                let bottom = if end == 0 {
                    0.0
                } else {
                    t.row_y[end - 1] + t.row_h[end - 1]
                };
                (cy - g.y) + bottom
            }
            NodeKind::Box { .. } if banded(node) => {
                let Some(band) = self.bands(i).into_iter().next() else {
                    return g.h;
                };
                let mut need = 0.0f32;
                for c in band {
                    let cy = self.tree.nodes[c as usize].geo.y;
                    need = need.max((cy - g.y) + self.head(c));
                }
                need
            }
            // Kept together up to its first forced break.
            NodeKind::Box { .. }
                if node.brk.inside == InsideMode::Avoid && self.forced[i as usize] =>
            {
                self.head_to_break(i)
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

    /// Height from the top of column box `i` to its first forced break.
    fn head_to_break(&self, i: NodeIdx) -> f32 {
        let g = self.tree.nodes[i as usize].geo;
        for c in self.tree.children(i).filter(|&c| self.in_flow(c)) {
            let ln = &self.tree.nodes[c as usize];
            let Some(node) = ln.node else { continue };
            if matches!(node.kind, NodeKind::PageBreak) || node.brk.before == BreakMode::Page {
                return ln.geo.y - g.y;
            }
            if self.forced[c as usize] {
                return (ln.geo.y - g.y) + self.head_to_break(c);
            }
            if node.brk.after == BreakMode::Page {
                return ln.geo.y + ln.geo.h - g.y;
            }
        }
        g.h
    }

    /// Children that take part in pagination (not absolutely positioned,
    /// not hidden).
    fn in_flow(&self, c: NodeIdx) -> bool {
        self.tree.nodes[c as usize].node.is_some_and(|n| {
            n.style.position != Some(Position::Absolute) && n.style.display != Some(Display::None)
        })
    }

    /// Places the children of column box `i` one by one. Out-of-flow
    /// children stay with the box top, at shift `top`.
    fn place_children(&mut self, i: NodeIdx, top: f32) {
        let kids: Vec<NodeIdx> = self.tree.children(i).collect();
        for c in kids {
            if self.in_flow(c) {
                self.place(c);
            } else {
                self.place_out_of_flow(c, top);
            }
        }
    }

    fn place_out_of_flow(&mut self, c: NodeIdx, top: f32) {
        let s = std::mem::replace(&mut self.shift, top);
        self.set_subtree(c);
        self.shift = s;
    }

    /// Places an in-flow node.
    fn place(&mut self, i: NodeIdx) {
        let ln = &self.tree.nodes[i as usize];
        let Some(node) = ln.node else { return };
        let g = ln.geo;
        let atomic = self.atomic(i, node);
        // The parent's break already moved this node to a new page.
        let taken = std::mem::take(&mut self.lead_taken);

        if matches!(node.kind, NodeKind::PageBreak) {
            self.set_subtree(i);
            if !taken {
                self.pending.get_or_insert(g.y + self.shift);
                self.pending_nodes.push(i);
            }
            return;
        }
        // A break before this node: one pending from the content before
        // it, or its own (or its first child's) `before`.
        let lead =
            !taken && (node.brk.before == BreakMode::Page || (!atomic && self.leads[i as usize]));
        let broke = match self.pending.take() {
            Some(y) => Some(y),
            None => lead.then_some(g.y + self.shift),
        };
        if let Some(y) = broke {
            self.push(y);
            for b in std::mem::take(&mut self.pending_nodes) {
                self.set_subtree(b);
            }
        }

        let top = g.y + self.shift;
        let need = self.head(i).max(node.brk.min_presence_ahead.unwrap_or(0.0));
        if top + need > self.page_end(top) + PAGED_EPS {
            self.push(top);
        }
        let top = g.y + self.shift;
        let fits = top + g.h <= self.page_end(top) + PAGED_EPS;

        if atomic || (fits && !self.forced[i as usize]) {
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
                NodeKind::Box { .. } if banded(node) => self.place_bands(i),
                NodeKind::Box { .. } => {
                    self.lead_taken = taken || broke.is_some();
                    self.place_children(i, self.shift);
                    self.lead_taken = false;
                }
                _ => {}
            }
            self.out.shift_bottom[i as usize] = self.shift;
        }

        if node.brk.after == BreakMode::Page {
            self.pending.get_or_insert(g.y + g.h + self.shift);
        }
    }

    /// Places a row, grid, wrapping or reversed box taller than a page band
    /// by band. A band moves to the next page when the head of one of its
    /// children does not fit; each child then paginates from the band top,
    /// like a table cell, and the band ends below the longest. Boxes that
    /// reach the band bottom (stretched) end with it.
    fn place_bands(&mut self, i: NodeIdx) {
        let box_shift = self.shift;
        let out_of_flow: Vec<NodeIdx> = self
            .tree
            .children(i)
            .filter(|&c| !self.in_flow(c))
            .collect();
        for c in out_of_flow {
            self.place_out_of_flow(c, box_shift);
        }
        let tree = self.tree;
        let geo = |c: NodeIdx| tree.nodes[c as usize].geo;
        for band in self.bands(i) {
            let top = geo(band[0]).y;
            let bottom = band
                .iter()
                .map(|&c| geo(c).y + geo(c).h)
                .fold(top, f32::max);
            let mut need = 0.0f32;
            for &c in &band {
                need = need.max(geo(c).y - top + self.head(c));
            }
            let paged = top + self.shift;
            if paged + need > self.page_end(paged) + PAGED_EPS {
                self.push(paged);
            }
            let start = self.shift;
            let mut end = start;
            for &c in &band {
                self.shift = start;
                self.place(c);
                end = end.max(self.shift);
            }
            for &c in &band {
                let g = geo(c);
                let is_box = tree.nodes[c as usize]
                    .node
                    .is_some_and(|n| matches!(n.kind, NodeKind::Box { .. }));
                if is_box && g.y + g.h >= bottom - PAGED_EPS {
                    self.out.shift_bottom[c as usize] = end;
                }
            }
            self.shift = end;
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
            // A single row taller than a page splits inside its cells.
            let splits = !is_header && group.len() == 1 && g_h + header_h > self.h + PAGED_EPS;
            if !is_header && (top - fresh_start).abs() > PAGED_EPS {
                let mut overflows =
                    top + g_h > self.page_end(top) + PAGED_EPS && !self.at_page_top(top);
                if splits && top + self.row_head(first) <= self.page_end(top) + PAGED_EPS {
                    // It starts here if the first line of every cell fits.
                    overflows = false;
                }
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
                }
            }
            if splits {
                self.split_row(first, header_h);
                continue;
            }
            if !is_header && g_h + header_h > self.h + PAGED_EPS {
                tracing::warn!(
                    height = g_h,
                    "rows tied by a row span taller than a page are sliced"
                );
            }
            for r in group.clone() {
                self.set_subtree(rows[r]);
            }
        }
    }

    /// Height from a row's top to the end of the first unbreakable piece
    /// of its tallest cell start.
    fn row_head(&mut self, row: NodeIdx) -> f32 {
        let row_y = self.tree.nodes[row as usize].geo.y;
        let cells: Vec<NodeIdx> = self.tree.children(row).collect();
        let mut need = 0.0f32;
        for cell in cells {
            if let Some(c) = self.tree.children(cell).find(|&c| self.in_flow(c)) {
                let cy = self.tree.nodes[c as usize].geo.y;
                need = need.max(cy - row_y + self.head(c));
            }
        }
        need
    }

    /// Splits a row taller than a page inside its cells: each cell's
    /// content paginates like a column from the row top, keeping room for
    /// the repeated header rows; the row ends below the longest cell.
    fn split_row(&mut self, row: NodeIdx, header_h: f32) {
        let start = self.shift;
        let mut end = start;
        let reserve = std::mem::replace(&mut self.reserve, header_h);
        let cells: Vec<NodeIdx> = self.tree.children(row).collect();
        for &cell in &cells {
            self.shift = start;
            self.out.shift_top[cell as usize] = start;
            self.place_children(cell, start);
            end = end.max(self.shift);
        }
        self.reserve = reserve;
        for &cell in &cells {
            self.out.shift_bottom[cell as usize] = end;
        }
        self.out.shift_top[row as usize] = start;
        self.out.shift_bottom[row as usize] = end;
        self.shift = end;
    }

    /// `(orphans, widows)` of a text node, at least 1 each.
    fn orphans_widows(node: &Node) -> (usize, usize) {
        let get = |v: Option<u32>| v.unwrap_or(1).max(1) as usize;
        (get(node.style.orphans), get(node.style.widows))
    }

    /// Places a paragraph line by line. A break before line `k` moves up
    /// when fewer than `widows` lines would follow it, as long as the page
    /// keeps `orphans` lines (CSS Fragmentation §4.4).
    fn split_text(&mut self, i: NodeIdx, node: &Node) {
        let g = self.tree.nodes[i as usize].geo;
        let Rect { y: cy, w: cw, .. } = g.content_box();
        let Some(layout) = self.lines.lines(node, cw) else {
            return;
        };
        let (orphans, widows) = Self::orphans_widows(node);
        let lines = &layout.lines;
        let n = lines.len();
        let mut shifts: Vec<f32> = Vec::with_capacity(n);
        // First line on the current page.
        let mut chunk = 0usize;
        let mut k = 0usize;
        while k < n {
            let top = cy + lines[k].top + self.shift;
            let overflows =
                top + lines[k].height > self.page_end(top) + PAGED_EPS && !self.at_page_top(top);
            // A line that lands exactly on a later page top breaks too.
            let new_page = k > chunk
                && self.page_of(top) > self.page_of(cy + lines[k - 1].top + shifts[k - 1]);
            if overflows || new_page {
                let wanted = n.saturating_sub(widows);
                if wanted < k && wanted >= chunk + orphans {
                    // Carry more lines over: replay from the earlier break.
                    self.shift = shifts[wanted];
                    shifts.truncate(wanted);
                    k = wanted;
                    self.push(cy + lines[k].top + self.shift);
                } else if overflows {
                    self.push(top);
                }
                chunk = k;
            }
            shifts.push(self.shift);
            k += 1;
        }
        self.out.line_shifts[i as usize] = shifts;
    }
}

#[cfg(test)]
mod tests;
