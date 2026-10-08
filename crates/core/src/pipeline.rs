//! The render pipeline after validation: layout → pagination → paint.

use std::collections::HashMap;

use rayon::prelude::*;
use taffy::{AvailableSpace, Size as TSize};

use crate::fonts::FontSet;
use crate::images::ImageSet;
use crate::layout::{Geometry, LayoutEngine, LeafMeasure, NodeIdx, TaffyLayout, Tree};
use crate::paginate::{paginate, Placement};
use crate::paint::{self, ContentCtx, ContentPainter, FlowPart, Op, PageContent, Rect};
use crate::schema::{Display, Document, Node, NodeKind, PageSpec};
use crate::text::{PageNumbers, RustybuzzShaper, TextCache, TextPainter};
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

/// Leaf measurement: text through the text cache, images by intrinsic size.
struct Leaves<'x, 's, 'f, 'a> {
    text: &'x mut Cache<'s, 'f, 'a>,
    images: &'x ImageSet,
}

impl LeafMeasure for Leaves<'_, '_, '_, '_> {
    fn measure(
        &mut self,
        node: &Node,
        known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32> {
        match node.kind {
            NodeKind::Image { .. } => self.images.measure(node, known, available),
            _ => self.text.measure(node, known, available),
        }
    }
}

/// Leaf painting: text and images.
struct Content<'x, T: ContentPainter> {
    text: T,
    images: &'x ImageSet,
}

impl<T: ContentPainter> ContentPainter for Content<'_, T> {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
        match node.kind {
            NodeKind::Image { .. } => self.images.paint(node, geo, ctx, ops),
            _ => self.text.paint(node, geo, ctx, ops),
        }
    }
}

/// One page template after layout and pagination.
struct Laid<'a> {
    spec: &'a PageSpec,
    flow: Tree<'a>,
    placement: Placement,
    header: Option<Tree<'a>>,
    footer: Option<Tree<'a>>,
}

fn layout_fixed<'a>(
    node: &'a Node,
    width: f32,
    leaves: &mut Leaves<'_, '_, '_, '_>,
) -> Result<Tree<'a>, InkoxideError> {
    let mut tree = Tree::build_single(node);
    TaffyLayout.compute(&mut tree, width, 0.0, leaves)?;
    Ok(tree)
}

/// Lays out and paginates every page template, then paints the pages in
/// parallel, handing each to `encode` as soon as it is painted.
pub(crate) fn run<'d, T: Send>(
    doc: &'d Document,
    fonts: &FontSet<'_>,
    text: &mut Cache<'_, '_, '_>,
    images: &ImageSet,
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
    text.prepare_all(&texts);
    drop(texts);

    let mut laid = Vec::with_capacity(doc.pages.len());
    for spec in &doc.pages {
        let _span = tracing::debug_span!("layout+paginate").entered();
        let m = spec.margin;
        let content_w = spec.size.width - m.left - m.right;
        let content_h = spec.size.height - m.top - m.bottom;
        let mut flow = Tree::build(&spec.children);
        let mut leaves = Leaves { text, images };
        TaffyLayout.compute(&mut flow, content_w, content_h, &mut leaves)?;
        let header = spec
            .header
            .as_deref()
            .map(|n| layout_fixed(n, spec.size.width, &mut leaves))
            .transpose()?;
        let footer = spec
            .footer
            .as_deref()
            .map(|n| layout_fixed(n, spec.size.width, &mut leaves))
            .transpose()?;
        let placement = paginate(&flow, content_h, text);
        laid.push(Laid {
            spec,
            flow,
            placement,
            header,
            footer,
        });
    }

    // Page numbers are known only now; placeholders are filled at paint.
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
    let pages = jobs
        .par_iter()
        .enumerate()
        .map(|(i, &(l, k))| {
            let numbers = PageNumbers {
                current: i as u32 + 1,
                total,
            };
            encode(paint_page(l, k, numbers, fonts, text, images))
        })
        .collect::<Result<_, _>>()?;
    Ok((pages, marks))
}

/// Destinations of the ids and bookmarks in one paginated flow, whose
/// first page is document page `first`. Headers and footers have none.
fn collect_marks<'d>(l: &Laid<'d>, first: u32, out: &mut Marks<'d>) {
    let (tree, pl, m) = (&l.flow, &l.placement, l.spec.margin);
    let mut stack: Vec<NodeIdx> = tree.children(0).rev().collect();
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
        let page = pl.page_range[i as usize].0;
        let top = ln.geo.y + pl.shift_top[i as usize] - page as f32 * pl.page_h;
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
    if node.style.display == Some(Display::None) {
        return;
    }
    match &node.kind {
        NodeKind::Text { .. } => out.push(node),
        NodeKind::Box { children } => {
            for c in children {
                collect_texts(c, out);
            }
        }
        NodeKind::Table(t) => {
            for c in t.rows.iter().flat_map(|r| &r.cells) {
                if c.style.display == Some(Display::None) {
                    continue;
                }
                for n in &c.children {
                    collect_texts(n, out);
                }
            }
        }
        _ => {}
    }
}

fn paint_page(
    l: &Laid<'_>,
    k: u32,
    numbers: PageNumbers,
    fonts: &FontSet<'_>,
    text: &Cache<'_, '_, '_>,
    images: &ImageSet,
) -> PageContent {
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
    let nums = (numbers.current, numbers.total);
    let mut painter = Content {
        text: TextPainter {
            cache: text,
            fonts,
            numbers,
        },
        images,
    };

    // Flow: paged space [k·H, (k+1)·H) mapped onto the content band,
    // clipped vertically (split boxes are sliced, like CSS
    // `box-decoration-break: slice`).
    let dy = m.top - k as f32 * h;
    page.ops.push(Op::Save);
    page.ops.push(Op::ClipRect(Rect::new(0.0, m.top, pw, h)));
    paint::paint_flow(
        &l.flow,
        FlowPart::Page {
            placement: &l.placement,
            page: k,
        },
        m.left,
        dy,
        nums,
        &mut painter,
        &mut page.ops,
    );
    page.ops.push(Op::Restore);
    if !l.placement.fixed.is_empty() {
        paint::paint_flow(
            &l.flow,
            FlowPart::Fixed {
                placement: &l.placement,
                page: k,
            },
            m.left,
            dy,
            nums,
            &mut painter,
            &mut page.ops,
        );
    }
    if let Some(header) = &l.header {
        paint::paint_flow(
            header,
            FlowPart::All,
            0.0,
            0.0,
            nums,
            &mut painter,
            &mut page.ops,
        );
    }
    if let Some(footer) = &l.footer {
        let fh = footer.nodes[0].geo.h;
        paint::paint_flow(
            footer,
            FlowPart::All,
            0.0,
            ph - fh,
            nums,
            &mut painter,
            &mut page.ops,
        );
    }
    page
}
