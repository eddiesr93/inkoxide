//! The render pipeline after validation: layout → pagination → paint.

mod estimate;
mod marks;
mod numbering;
mod page;
mod pdfa;

use estimate::repeated_size;
use marks::collect_marks;
pub(crate) use marks::{Dest, Marks, OutlineEntry};
use numbering::{number_flows, relayout};
use page::paint_page;
use pdfa::{transparent_color_glyph, transparent_svg_color_glyph};

use rayon::prelude::*;

use crate::fonts::FontSet;
use crate::images::ImageSet;
use crate::layout::{
    AvailableSpace, LayoutEngine, LeafMeasure, LineBoxes, Size, TaffyLayout, Tree, Wrapped,
};
use crate::paginate::{paginate, Placement};
use crate::paint::PageContent;
use crate::pdf::Profile;
use crate::schema::{Document, Node, NodeKind, PageSpec, Position};
use crate::text::{ColorGlyphs, PageNumbers, RustybuzzShaper, TextCache};
use crate::InkoxideError;

/// Most pages one render may produce: a memory guard against runaway
/// sizes (a 10¹¹ pt box), checked before anything is painted. Far above
/// any real document; ~2 GB of output at typical page sizes.
pub(crate) const MAX_PAGES: u32 = 100_000;

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
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        match node.kind {
            NodeKind::Image { .. } => self.images.measure(node, known, available),
            NodeKind::Field(ref f) => crate::forms::measure(f, available),
            // It takes no space.
            NodeKind::Note(_) => Size::ZERO,
            _ => self.text.measure(node, known, available),
        }
    }

    fn first_baseline(&mut self, node: &Node, width: f32) -> Option<f32> {
        match node.kind {
            NodeKind::Text { .. } => self.text.first_baseline(node, width),
            _ => None,
        }
    }

    fn measure_lines(&mut self, node: &Node, boxes: &LineBoxes) -> Wrapped {
        self.text.measure_lines(node, boxes)
    }
}

/// One page template after layout and pagination.
struct Laid<'a> {
    spec: &'a PageSpec,
    /// The flow, without its overlays.
    flow: Tree<'a>,
    placement: Placement,
    header: Option<PageBand<'a>>,
    footer: Option<PageBand<'a>>,
    /// The flow's overlays, with their index in `spec.children`.
    overlays: Vec<(usize, PageBand<'a>)>,
}

/// A header, a footer or an overlay, laid out once with stand-in digits for
/// page numbers. One that shows page numbers is laid out again on every
/// page, with that page's numbers, so its lines break and its boxes size to
/// the digits painted; the first layout then only warms the text cache.
struct PageBand<'a> {
    node: &'a Node,
    tree: Tree<'a>,
    numbered: bool,
    /// The box it is laid out in: the page width for headers and footers,
    /// the page content box for overlays.
    width: f32,
    height: f32,
}

impl<'a> PageBand<'a> {
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
/// calling thread alone. When `profile` forbids transparency (PDF/A-1b),
/// a colour glyph that needs it fails the render, at the path of its run
/// or SVG image. Returns the pages, the marks and the colour glyphs, which
/// SVG text draws too.
pub(crate) fn run<'d, T: Send>(
    doc: &'d Document,
    fonts: &FontSet<'_>,
    text: &mut Cache<'_, '_, '_>,
    images: &mut ImageSet,
    parallel: bool,
    profile: Profile,
    encode: impl Fn(PageContent) -> Result<T, InkoxideError> + Sync,
) -> Result<(Vec<T>, Marks<'d>, ColorGlyphs), InkoxideError> {
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
    // A render whose shaping failed fails: nothing more to lay out.
    if let Some(e) = text.shaping_error() {
        return Err(e);
    }
    let color = |k| fonts.get(k).has_color();
    let mut used = text.glyph_colors(&texts, color);
    drop(texts);
    // SVG text draws colour glyphs in black.
    for (k, glyphs) in images.svg_text_glyphs(color) {
        for g in glyphs {
            used.entry((k, g))
                .or_default()
                .insert(crate::fonts::BLACK_TEXT);
        }
    }
    let colors = ColorGlyphs::load(fonts, &used, images, parallel);
    if !profile.transparency() {
        let conflict = transparent_color_glyph(doc, fonts, text, images, &colors)
            .or_else(|| transparent_svg_color_glyph(doc, fonts, images, &colors));
        if let Some(conflict) = conflict {
            return Err(conflict.into());
        }
    }
    let images: &ImageSet = images;

    let mut laid = Vec::with_capacity(doc.pages.len());
    for spec in &doc.pages {
        let _span = tracing::debug_span!("layout+paginate").entered();
        let (content_w, content_h) = spec.content_size();
        let (flow, placement) = lay_out_flow(spec, text, images)?;
        let mut leaves = Leaves { text, images };
        let header = spec
            .header
            .as_deref()
            .map(|n| PageBand::new(n, spec.size.width, 0.0, &mut leaves))
            .transpose()?;
        let footer = spec
            .footer
            .as_deref()
            .map(|n| PageBand::new(n, spec.size.width, 0.0, &mut leaves))
            .transpose()?;
        let overlays = spec
            .children
            .iter()
            .enumerate()
            .filter(|(_, n)| is_overlay(n))
            .map(|(k, n)| Ok((k, PageBand::new(n, content_w, content_h, &mut leaves)?)))
            .collect::<Result<_, InkoxideError>>()?;
        laid.push(Laid {
            spec,
            flow,
            placement,
            header,
            footer,
            overlays,
        });
    }
    page_count(&laid)?;
    number_flows(&mut laid, text, images)?;

    // The painter draws each page's digits in placeholders, and headers
    // and footers that show them are laid out per page.
    let total = page_count(&laid)?;
    repeated_size(&laid, text)?;
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
        encode(paint_page(l, k, numbers, fonts, text, images, &colors)?)
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
    Ok((pages, marks, colors))
}

/// Lays out and paginates the flow of page template `spec`: its children
/// other than overlays.
fn lay_out_flow<'a>(
    spec: &'a PageSpec,
    text: &mut Cache<'_, '_, '_>,
    images: &ImageSet,
) -> Result<(Tree<'a>, Placement), InkoxideError> {
    let (content_w, content_h) = spec.content_size();
    let mut flow = Tree::build(spec.children.iter().filter(|n| !is_overlay(n)));
    let mut leaves = Leaves { text, images };
    TaffyLayout.compute(&mut flow, content_w, content_h, &mut leaves)?;
    let placement = paginate(&flow, content_h, text);
    Ok((flow, placement))
}

/// The document's page count, within [`MAX_PAGES`].
fn page_count(laid: &[Laid<'_>]) -> Result<u32, InkoxideError> {
    let total: u64 = laid.iter().map(|l| u64::from(l.placement.pages)).sum();
    if total > u64::from(MAX_PAGES) {
        return Err(InkoxideError::Layout(format!(
            "the document needs {total} pages; the limit is {MAX_PAGES}"
        )));
    }
    Ok(total as u32)
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod debug_tests;
