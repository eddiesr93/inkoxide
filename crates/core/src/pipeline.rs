//! The render pipeline after validation: layout → pagination → paint.

use std::collections::HashMap;

use rayon::prelude::*;
use taffy::{AvailableSpace, Size as TSize};

use crate::fonts::FontSet;
use crate::images::ImageSet;
use crate::layout::{Geometry, LayoutEngine, LeafMeasure, NodeIdx, TaffyLayout, Tree};
use crate::paginate::{paginate, Placement};
use crate::paint::{self, ContentCtx, ContentPainter, DebugBoxes, FlowPart, Op, PageContent, Rect};
use crate::schema::{Display, Document, Node, NodeKind, PageSpec, Position};
use crate::text::{PageNumbers, PageText, RustybuzzShaper, TextCache, TextPainter};
use crate::InkoxideError;

/// Most pages one render may produce: a memory guard against runaway
/// sizes (a 10¹¹ pt box), checked before anything is painted. Far above
/// any real document; ~2 GB of output at typical page sizes.
pub(crate) const MAX_PAGES: u32 = 100_000;

/// A link or outline destination: a page and the top-left corner of the
/// target on it, in PDF coordinates (origin bottom-left, y up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dest {
    pub page: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutlineEntry<'d> {
    pub title: &'d str,
    pub level: u8,
    pub dest: Dest,
}

/// Destinations found after pagination: the first displayed node of each
/// id, and the bookmarks in document order.
#[derive(Debug, Default)]
pub(crate) struct Marks<'d> {
    pub anchors: HashMap<&'d str, Dest>,
    pub outline: Vec<OutlineEntry<'d>>,
}

type Cache<'s, 'f, 'a> = TextCache<'s, RustybuzzShaper<'f, 'a>>;

/// Leaf measurement: text through `T` (the text cache, or one page's
/// text), images by intrinsic size.
struct Leaves<'x, T: LeafMeasure> {
    text: &'x mut T,
    images: &'x ImageSet,
}

impl<T: LeafMeasure> LeafMeasure for Leaves<'_, T> {
    fn measure(
        &mut self,
        node: &Node,
        known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32> {
        match node.kind {
            NodeKind::Image { .. } => self.images.measure(node, known, available),
            NodeKind::Field(ref f) => crate::forms::measure(f, available),
            _ => self.text.measure(node, known, available),
        }
    }
}

/// Leaf painting: text through `T`, images and form fields.
struct Content<'x, T: ContentPainter> {
    text: T,
    images: &'x ImageSet,
    fonts: &'x FontSet<'x>,
}

impl<T: ContentPainter> ContentPainter for Content<'_, T> {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
        match node.kind {
            NodeKind::Image { .. } => self.images.paint(node, geo, ctx, ops),
            NodeKind::Field(ref f) => crate::forms::paint(f, geo, ctx, self.fonts, ops),
            _ => self.text.paint(node, geo, ctx, ops),
        }
    }

    fn numbers(&self) -> PageNumbers {
        self.text.numbers()
    }
}

/// One page template after layout and pagination.
struct Laid<'a> {
    spec: &'a PageSpec,
    /// The flow, without its overlays.
    flow: Tree<'a>,
    placement: Placement,
    header: Option<Band<'a>>,
    footer: Option<Band<'a>>,
    /// The flow's overlays, with their index in `spec.children`.
    overlays: Vec<(usize, Band<'a>)>,
}

/// A header, a footer or an overlay, laid out once with stand-in digits for
/// page numbers. One that shows page numbers is laid out again on every
/// page, with that page's numbers, so its lines break and its boxes size to
/// the digits painted; the first layout then only warms the text cache.
struct Band<'a> {
    node: &'a Node,
    tree: Tree<'a>,
    numbered: bool,
    /// The box it is laid out in: the page width for headers and footers,
    /// the page content box for overlays.
    width: f32,
    height: f32,
}

impl<'a> Band<'a> {
    fn new(
        node: &'a Node,
        width: f32,
        height: f32,
        leaves: &mut dyn LeafMeasure,
    ) -> Result<Self, InkoxideError> {
        let mut texts = Vec::new();
        collect_texts(node, &mut texts);
        let numbered = texts.iter().any(|n| match &n.kind {
            NodeKind::Text { runs } => runs.iter().any(|r| r.placeholder.is_some()),
            _ => false,
        });
        Ok(Self {
            node,
            tree: layout_fixed(node, width, height, leaves)?,
            numbered,
            width,
            height,
        })
    }
}

/// Whether a node of a page's `children` is an overlay: fixed and
/// absolutely positioned. An overlay is laid out on its own, against the
/// page content box, and painted on every page of the flow; it takes no
/// space in the flow.
pub(crate) fn is_overlay(node: &Node) -> bool {
    node.brk.fixed && node.style.position == Some(Position::Absolute)
}

/// Lays out `node` alone in a box `width` wide and at least `height` high
/// (an absolutely positioned node is placed against that box).
fn layout_fixed<'a>(
    node: &'a Node,
    width: f32,
    height: f32,
    leaves: &mut dyn LeafMeasure,
) -> Result<Tree<'a>, InkoxideError> {
    let mut tree = Tree::build_single(node);
    TaffyLayout.compute(&mut tree, width, height, leaves)?;
    Ok(tree)
}

/// Lays out and paginates every page template, then paints the pages,
/// handing each to `encode` as soon as it is painted. With `parallel`,
/// shaping and painting run on the current rayon pool; without it, on the
/// calling thread alone.
pub(crate) fn run<'d, T: Send>(
    doc: &'d Document,
    fonts: &FontSet<'_>,
    text: &mut Cache<'_, '_, '_>,
    images: &ImageSet,
    parallel: bool,
    encode: impl Fn(PageContent) -> Result<T, InkoxideError> + Sync,
) -> Result<(Vec<T>, Marks<'d>), InkoxideError> {
    // Shaping is the bulk of layout and independent per paragraph: do it
    // up front, in parallel.
    let mut texts = Vec::new();
    for spec in &doc.pages {
        let fixed = spec.header.iter().chain(&spec.footer).map(|b| &**b);
        for n in fixed.chain(&spec.children) {
            collect_texts(n, &mut texts);
        }
    }
    text.prepare_all(&texts, parallel);
    drop(texts);

    let mut laid = Vec::with_capacity(doc.pages.len());
    for spec in &doc.pages {
        let _span = tracing::debug_span!("layout+paginate").entered();
        let m = spec.margin;
        let content_w = spec.size.width - m.left - m.right;
        let content_h = spec.size.height - m.top - m.bottom;
        let mut flow = Tree::build(spec.children.iter().filter(|n| !is_overlay(n)));
        let mut leaves = Leaves { text, images };
        TaffyLayout.compute(&mut flow, content_w, content_h, &mut leaves)?;
        let header = spec
            .header
            .as_deref()
            .map(|n| Band::new(n, spec.size.width, 0.0, &mut leaves))
            .transpose()?;
        let footer = spec
            .footer
            .as_deref()
            .map(|n| Band::new(n, spec.size.width, 0.0, &mut leaves))
            .transpose()?;
        let overlays = spec
            .children
            .iter()
            .enumerate()
            .filter(|(_, n)| is_overlay(n))
            .map(|(k, n)| Ok((k, Band::new(n, content_w, content_h, &mut leaves)?)))
            .collect::<Result<_, InkoxideError>>()?;
        let placement = paginate(&flow, content_h, text);
        laid.push(Laid {
            spec,
            flow,
            placement,
            header,
            footer,
            overlays,
        });
    }

    // Page numbers are known only now: placeholders are filled at paint,
    // and headers and footers that show them are laid out per page.
    let total: u64 = laid.iter().map(|l| u64::from(l.placement.pages)).sum();
    if total > u64::from(MAX_PAGES) {
        return Err(InkoxideError::Layout(format!(
            "the document needs {total} pages; the limit is {MAX_PAGES}"
        )));
    }
    let total = total as u32;
    let mut marks = Marks::default();
    let mut first = 0;
    for l in &laid {
        collect_marks(l, first, &mut marks);
        first += l.placement.pages;
    }
    let jobs: Vec<(&Laid<'_>, u32)> = laid
        .iter()
        .flat_map(|l| (0..l.placement.pages).map(move |k| (l, k)))
        .collect();
    let text: &Cache<'_, '_, '_> = text;
    let page = |(i, &(l, k)): (usize, &(&Laid<'_>, u32))| {
        let numbers = PageNumbers {
            current: i as u32 + 1,
            total,
        };
        encode(paint_page(l, k, numbers, fonts, text, images)?)
    };
    let pages = if parallel {
        jobs.par_iter()
            .enumerate()
            .map(page)
            .collect::<Result<_, _>>()?
    } else {
        jobs.iter()
            .enumerate()
            .map(page)
            .collect::<Result<_, _>>()?
    };
    Ok((pages, marks))
}

/// Destinations of the ids and bookmarks in one paginated flow, whose
/// first page is document page `first`, in document order. Headers and
/// footers have none; an overlay's point to where it is painted on the
/// flow's first page.
fn collect_marks<'d>(l: &Laid<'d>, first: u32, out: &mut Marks<'d>) {
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
fn subtree_marks<'d>(
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
        let Some(node) = ln.node else {
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
            });
        }
    }
}

/// Text nodes under `node`, in document order.
fn collect_texts<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    if node.hidden() {
        return;
    }
    if let NodeKind::Text { .. } = node.kind {
        out.push(node);
    }
    for c in node.shown_children() {
        collect_texts(c, out);
    }
}

/// Lays out a header, footer or overlay that shows page numbers again,
/// measuring with one page's text.
fn relayout<'a>(
    band: Option<&Band<'a>>,
    leaves: &mut dyn LeafMeasure,
) -> Result<Option<Tree<'a>>, InkoxideError> {
    match band {
        Some(b) if b.numbered => layout_fixed(b.node, b.width, b.height, leaves).map(Some),
        _ => Ok(None),
    }
}

fn paint_page(
    l: &Laid<'_>,
    k: u32,
    numbers: PageNumbers,
    fonts: &FontSet<'_>,
    text: &Cache<'_, '_, '_>,
    images: &ImageSet,
) -> Result<PageContent, InkoxideError> {
    let spec = l.spec;
    let (pw, ph) = (spec.size.width, spec.size.height);
    let m = spec.margin;
    let h = l.placement.page_h;
    let mut page = PageContent::new(pw, ph);
    if let Some(bg) = spec.background {
        page.ops.push(Op::FillRect {
            rect: Rect::new(0.0, 0.0, pw, ph),
            color: bg,
        });
    }
    // Headers, footers and overlays with page numbers, laid out for this
    // page.
    let mut page_text = PageText::new(text, numbers);
    let mut leaves = Leaves {
        text: &mut page_text,
        images,
    };
    let header_here = relayout(l.header.as_ref(), &mut leaves)?;
    let footer_here = relayout(l.footer.as_ref(), &mut leaves)?;
    let overlays_here = l
        .overlays
        .iter()
        .map(|(_, o)| relayout(Some(o), &mut leaves))
        .collect::<Result<Vec<_>, _>>()?;
    let header = header_here.as_ref().or(l.header.as_ref().map(|b| &b.tree));
    let footer = footer_here.as_ref().or(l.footer.as_ref().map(|b| &b.tree));
    let mut painter = Content {
        text: TextPainter {
            cache: text,
            page: None,
            fonts,
            numbers,
        },
        images,
        fonts,
    };

    // `debug` outlines, drawn over everything once the page is painted.
    let mut debug = DebugBoxes::new(spec.debug);

    // Overlays: placed in the content box, like the flow, but not clipped.
    // In paint order, by `zIndex` then document order: those with a
    // negative `zIndex` under the flow, the others over it.
    let mut overlays: Vec<usize> = (0..l.overlays.len()).collect();
    let z = |i: usize| l.overlays[i].1.node.style.z_index.unwrap_or(0);
    overlays.sort_by_key(|&i| z(i));
    let (under, over) = overlays.split_at(overlays.partition_point(|&i| z(i) < 0));
    let paint_overlays = |which: &[usize],
                          painter: &mut dyn ContentPainter,
                          ops: &mut Vec<Op>,
                          debug: &mut DebugBoxes| {
        for &i in which {
            let tree = overlays_here[i].as_ref().unwrap_or(&l.overlays[i].1.tree);
            paint::paint_flow(tree, FlowPart::All, m.left, m.top, painter, ops, debug);
        }
    };
    painter.text.page = Some(&page_text);
    paint_overlays(under, &mut painter, &mut page.ops, &mut debug);
    painter.text.page = None;
    let under_boxes = std::mem::take(&mut debug.boxes);

    // Flow: paged space [k·H, (k+1)·H) mapped onto the content band,
    // clipped vertically (split boxes are sliced, like CSS
    // `box-decoration-break: slice`).
    let dy = m.top - k as f32 * h;
    let band = Rect::new(0.0, m.top, pw, h);
    page.ops.push(Op::Save);
    page.ops.push(Op::ClipRect(band));
    paint::paint_flow(
        &l.flow,
        FlowPart::Page {
            placement: &l.placement,
            page: k,
        },
        m.left,
        dy,
        &mut painter,
        &mut page.ops,
        &mut debug,
    );
    page.ops.push(Op::Restore);
    let flow_boxes = std::mem::take(&mut debug.boxes);
    if !l.placement.fixed.is_empty() {
        paint::paint_flow(
            &l.flow,
            FlowPart::Fixed {
                placement: &l.placement,
                page: k,
            },
            m.left,
            dy,
            &mut painter,
            &mut page.ops,
            &mut debug,
        );
    }
    painter.text.page = Some(&page_text);
    paint_overlays(over, &mut painter, &mut page.ops, &mut debug);
    if let Some(header) = header {
        paint::paint_flow(
            header,
            FlowPart::All,
            0.0,
            0.0,
            &mut painter,
            &mut page.ops,
            &mut debug,
        );
    }
    if let Some(footer) = footer {
        let fh = footer.nodes[0].geo.h;
        paint::paint_flow(
            footer,
            FlowPart::All,
            0.0,
            ph - fh,
            &mut painter,
            &mut page.ops,
            &mut debug,
        );
    }

    let mut labels = paint::DebugLabels::default();
    if spec.debug {
        let page_box = paint::debug_page_box(pw, ph, m);
        paint::draw_debug(std::slice::from_ref(&page_box), &mut page.ops, &mut labels);
    }
    paint::draw_debug(&under_boxes, &mut page.ops, &mut labels);
    if !flow_boxes.is_empty() {
        // Sliced like the flow they outline.
        page.ops.push(Op::Save);
        page.ops.push(Op::ClipRect(band));
        paint::draw_debug(&flow_boxes, &mut page.ops, &mut labels);
        page.ops.push(Op::Restore);
    }
    paint::draw_debug(&debug.boxes, &mut page.ops, &mut labels);
    Ok(page)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod debug_tests;
