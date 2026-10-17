//! Page numbers: flows laid out again until the page count settles, and
//! headers, footers and overlays laid out with each page's numbers.

use super::{lay_out_flow, layout_fixed, page_count, Cache, Laid, PageBand};
use crate::images::ImageSet;
use crate::layout::{LeafMeasure, NodeIdx, Tree};
use crate::schema::{Display, Node};
use crate::text::PageNumbers;
use crate::InkoxideError;

/// Most times the flows are laid out again for the page numbers their text
/// shows. Numbers rarely move a paragraph to another page; if they keep
/// doing so, the last layout stands and the painter still draws each
/// page's own digits.
pub(super) const NUMBER_PASSES: usize = 3;

/// Lays out flow text that shows page numbers with the numbers of the page
/// it lands on, instead of the stand-in digits: the flows with such text
/// are laid out and paginated again, until no number changes.
pub(super) fn number_flows(
    laid: &mut [Laid<'_>],
    text: &mut Cache<'_, '_, '_>,
    images: &ImageSet,
) -> Result<(), InkoxideError> {
    for _ in 0..NUMBER_PASSES {
        let total = page_count(laid)?;
        let mut first = 0;
        let mut found = Vec::new();
        for (k, l) in laid.iter().enumerate() {
            for (node, page) in numbered_text(l, text) {
                let current = first + page + 1;
                found.push((k, node, PageNumbers { current, total }));
            }
            first += l.placement.pages;
        }
        let mut changed = vec![false; laid.len()];
        for (k, node, numbers) in found {
            changed[k] |= text.set_numbers(node, numbers);
        }
        if !changed.contains(&true) {
            break;
        }
        for (l, _) in laid.iter_mut().zip(changed).filter(|(_, c)| *c) {
            let _span = tracing::debug_span!("layout+paginate with page numbers").entered();
            (l.flow, l.placement) = lay_out_flow(l.spec, text, images)?;
        }
    }
    Ok(())
}

/// The flow paragraphs of `l` that show page numbers, with the page of
/// the flow their first number is on. Text in a fixed node or in a table's
/// header rows is painted on other pages too: it keeps the stand-in
/// digits' width, and the painter draws each page's own.
pub(super) fn numbered_text<'a>(
    l: &Laid<'a>,
    text: &mut Cache<'_, '_, '_>,
) -> Vec<(&'a Node, u32)> {
    let (tree, pl) = (&l.flow, &l.placement);
    let mut repeated = vec![false; tree.nodes.len()];
    let mut out = Vec::new();
    // Breadth-first: a parent comes before its children.
    for i in 1..tree.nodes.len() {
        let ln = &tree.nodes[i];
        let parent = ln.parent as usize;
        let header_row = tree
            .tables
            .get(&ln.parent)
            .is_some_and(|t| i - (tree.nodes[parent].children.start as usize) < t.header_rows);
        let fixed = ln.node().is_some_and(|n| n.brk.fixed);
        repeated[i] = repeated[parent] || header_row || fixed;
        let Some(node) = ln.node() else {
            continue;
        };
        if repeated[i] || ln.style().display == Some(Display::None) {
            continue;
        }
        let Some(glyph) = text.placeholder_glyph(node) else {
            continue;
        };
        let content = ln.geo.content_box();
        let Some(lines) = text.layout_in(node, &tree.line_boxes(i as NodeIdx)) else {
            continue;
        };
        let Some(li) = lines
            .lines
            .iter()
            .position(|line| glyph < line.end)
            .or(lines.lines.len().checked_sub(1))
        else {
            continue;
        };
        let i = i as NodeIdx;
        let top = content.y + lines.lines[li].top + pl.line_shift(i, li);
        let page = pl.page_of(top);
        out.push((
            node,
            page.clamp(pl.first_page(i), pl.pages.saturating_sub(1)),
        ));
    }
    out
}

/// Lays out a header, footer or overlay that shows page numbers again,
/// measuring with one page's text.
pub(super) fn relayout<'a>(
    band: Option<&PageBand<'a>>,
    leaves: &mut dyn LeafMeasure,
) -> Result<Option<Tree<'a>>, InkoxideError> {
    match band {
        Some(b) if b.numbered => layout_fixed(b.node, b.width, b.height, leaves).map(Some),
        _ => Ok(None),
    }
}
