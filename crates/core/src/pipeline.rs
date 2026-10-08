//! The render pipeline after validation: layout → pagination → paint.

use crate::fonts::FontSet;
use crate::layout::{LayoutEngine, TaffyLayout, Tree};
use crate::paginate::{paginate, Placement};
use crate::paint::{self, FlowPart, Op, PageContent, Rect};
use crate::schema::{Document, PageSpec};
use crate::text::{PageNumbers, RustybuzzShaper, TextCache, TextPainter};
use crate::InkoxideError;

type Cache<'s, 'f, 'a> = TextCache<'s, RustybuzzShaper<'f, 'a>>;

/// One page template after layout and pagination.
struct Laid<'a> {
    spec: &'a PageSpec,
    flow: Tree<'a>,
    placement: Placement,
    header: Option<Tree<'a>>,
    footer: Option<Tree<'a>>,
}

fn layout_fixed<'a>(
    node: &'a crate::schema::Node,
    width: f32,
    text: &mut Cache<'_, '_, '_>,
) -> Result<Tree<'a>, InkoxideError> {
    let mut tree = Tree::build_single(node);
    TaffyLayout.compute(&mut tree, width, 0.0, text)?;
    Ok(tree)
}

pub(crate) fn run(
    doc: &Document,
    fonts: &FontSet<'_>,
    text: &mut Cache<'_, '_, '_>,
) -> Result<Vec<PageContent>, InkoxideError> {
    let mut laid = Vec::with_capacity(doc.pages.len());
    for spec in &doc.pages {
        let _span = tracing::debug_span!("layout+paginate").entered();
        let m = spec.margin;
        let content_w = spec.size.width - m.left - m.right;
        let content_h = spec.size.height - m.top - m.bottom;
        let mut flow = Tree::build(&spec.children);
        TaffyLayout.compute(&mut flow, content_w, content_h, text)?;
        let placement = paginate(&flow, content_h, text);
        let header = spec
            .header
            .as_deref()
            .map(|n| layout_fixed(n, spec.size.width, text))
            .transpose()?;
        let footer = spec
            .footer
            .as_deref()
            .map(|n| layout_fixed(n, spec.size.width, text))
            .transpose()?;
        laid.push(Laid {
            spec,
            flow,
            placement,
            header,
            footer,
        });
    }

    // Page numbers are known only now; placeholders are filled at paint.
    let total: u32 = laid.iter().map(|l| l.placement.pages).sum();
    let mut pages = Vec::with_capacity(total as usize);
    let mut number = 0u32;
    for l in &laid {
        for k in 0..l.placement.pages {
            number += 1;
            let numbers = PageNumbers {
                current: number,
                total,
            };
            pages.push(paint_page(l, k, numbers, fonts, text));
        }
    }
    Ok(pages)
}

fn paint_page(
    l: &Laid<'_>,
    k: u32,
    numbers: PageNumbers,
    fonts: &FontSet<'_>,
    text: &mut Cache<'_, '_, '_>,
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
    let mut painter = TextPainter {
        cache: text,
        fonts,
        numbers,
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
            &mut painter,
            &mut page.ops,
        );
    }
    if let Some(header) = &l.header {
        paint::paint_flow(header, FlowPart::All, 0.0, 0.0, &mut painter, &mut page.ops);
    }
    if let Some(footer) = &l.footer {
        let fh = footer.nodes[0].geo.h;
        paint::paint_flow(
            footer,
            FlowPart::All,
            0.0,
            ph - fh,
            &mut painter,
            &mut page.ops,
        );
    }
    page
}
