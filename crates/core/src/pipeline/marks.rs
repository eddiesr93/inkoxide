//! Link destinations and the outline, collected from where each node
//! lands.

use std::collections::HashMap;

use super::Laid;
use crate::layout::{NodeIdx, Tree};
use crate::schema::Display;

/// A link or outline destination: a page and the top-left corner of the
/// target on it, in PDF coordinates (origin bottom-left, y up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dest {
    pub page: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone)]
pub(crate) struct OutlineEntry<'d> {
    pub title: &'d str,
    pub level: u8,
    pub dest: Dest,
    /// The node with the bookmark.
    pub node: &'d crate::schema::Node,
}

/// Without the node, which says nothing the rest does not.
impl std::fmt::Debug for OutlineEntry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutlineEntry")
            .field("title", &self.title)
            .field("level", &self.level)
            .field("dest", &self.dest)
            .finish_non_exhaustive()
    }
}

/// Destinations found after pagination: the first displayed node of each
/// id, and the bookmarks in document order.
#[derive(Debug, Default)]
pub(crate) struct Marks<'d> {
    pub anchors: HashMap<&'d str, Dest>,
    pub outline: Vec<OutlineEntry<'d>>,
}

/// Destinations of the ids and bookmarks in one paginated flow, whose
/// first page is document page `first`, in document order. Headers and
/// footers have none; an overlay's point to where it is painted on the
/// flow's first page.
pub(super) fn collect_marks<'d>(l: &Laid<'d>, first: u32, out: &mut Marks<'d>) {
    let (flow, pl) = (&l.flow, &l.placement);
    let mut flow_nodes = flow.children(0);
    let mut overlays = l.overlays.iter().peekable();
    for k in 0..l.spec.children.len() {
        if let Some((_, o)) = overlays.next_if(|(i, _)| *i == k) {
            subtree_marks(l, &o.tree, 1, first, |_, y| (0, y), out);
        } else if let Some(c) = flow_nodes.next() {
            subtree_marks(l, flow, c, first, |i, y| pl.locate(i, y), out);
        }
    }
}

/// Destinations of the ids and bookmarks under node `root` of `tree`.
/// `at(i, y)` is the page of node `i` in its flow, and its top `y` on that
/// page.
pub(super) fn subtree_marks<'d>(
    l: &Laid<'d>,
    tree: &Tree<'d>,
    root: NodeIdx,
    first: u32,
    at: impl Fn(NodeIdx, f32) -> (u32, f32),
    out: &mut Marks<'d>,
) {
    let m = l.spec.margin;
    let mut stack = vec![root];
    while let Some(i) = stack.pop() {
        let ln = &tree.nodes[i as usize];
        if ln.style().display == Some(Display::None) {
            continue;
        }
        stack.extend(tree.children(i).rev());
        let Some(node) = ln.node() else {
            continue;
        };
        if node.id.is_none() && node.bookmark.is_none() {
            continue;
        }
        let (page, top) = at(i, ln.geo.y);
        let dest = Dest {
            page: first + page,
            x: ln.geo.x + m.left,
            y: (l.spec.size.height - m.top - top).clamp(0.0, l.spec.size.height),
        };
        if let Some(id) = &node.id {
            out.anchors.entry(id.as_str()).or_insert(dest);
        }
        if let Some(b) = &node.bookmark {
            out.outline.push(OutlineEntry {
                title: &b.title,
                level: b.level,
                dest,
                node,
            });
        }
    }
}
