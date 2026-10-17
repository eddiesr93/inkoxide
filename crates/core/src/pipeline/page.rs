//! Painting one page: its flow, header, footer and overlays.

use super::{relayout, Cache, Laid, Leaves};
use crate::fonts::FontSet;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::images::ImageSet;
use crate::layout::Geometry;
use crate::paint::{
    self, ContentCtx, ContentPainter, DebugBoxes, DebugLine, FlowPart, Op, PageContent,
};
use crate::schema::{Node, NodeKind};
use crate::text::{ColorGlyphs, PageNumbers, PageText, TextPainter};
use crate::InkoxideError;

/// Leaf painting: text through `T`; images and form fields.
pub(super) struct Content<'x, T: ContentPainter> {
    pub(super) text: T,
    pub(super) images: &'x ImageSet,
    pub(super) fonts: &'x FontSet<'x>,
}

impl<T: ContentPainter> ContentPainter for Content<'_, T> {
    fn paint(&mut self, node: &Node, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
        match node.kind {
            NodeKind::Image { .. } => self.images.paint(node, geo, ctx, self.fonts, ops),
            NodeKind::Field(ref f) => crate::forms::paint(f, geo, ctx, self.fonts, ops),
            NodeKind::Note(ref n) => crate::notes::paint(Icon::Note(n), geo, ctx, ops),
            NodeKind::Attachment(ref a) => crate::notes::paint(Icon::Attachment(a), geo, ctx, ops),
            _ => self.text.paint(node, geo, ctx, ops),
        }
    }

    fn paint_outlined(
        &mut self,
        node: &Node,
        geo: &Geometry,
        ctx: &ContentCtx<'_>,
        ops: &mut Vec<Op>,
        lines: &mut Vec<DebugLine>,
    ) {
        match node.kind {
            NodeKind::Text { .. } => self.text.paint_outlined(node, geo, ctx, ops, lines),
            _ => self.paint(node, geo, ctx, ops),
        }
    }

    fn background(
        &mut self,
        node: &Node,
        padding_box: Rect,
        radii: &paint::Radii,
        ops: &mut Vec<Op>,
    ) {
        if let (Some(bg), Some(key)) = (&node.background_image, self.images.background_key(node)) {
            let placement = crate::images::ImagePlacement::new(bg.fit, bg.object_position);
            self.images
                .paint_in(key, padding_box, placement, radii, ops);
        }
    }

    fn numbers(&self) -> PageNumbers {
        self.text.numbers()
    }
}

pub(super) fn paint_page(
    l: &Laid<'_>,
    k: u32,
    numbers: PageNumbers,
    fonts: &FontSet<'_>,
    text: &Cache<'_, '_, '_>,
    images: &ImageSet,
    colors: &ColorGlyphs,
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
            images,
            colors,
            numbers,
            page_box: Rect::new(0.0, 0.0, pw, ph),
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
    // clipped vertically; its fixed nodes, in paint order, are not.
    let dy = m.top - k as f32 * h;
    let band = Rect::new(0.0, m.top, pw, h);
    paint::paint_flow(
        &l.flow,
        FlowPart::Page {
            placement: &l.placement,
            page: k,
            band,
        },
        m.left,
        dy,
        &mut painter,
        &mut page.ops,
        &mut debug,
    );
    let flow_boxes = std::mem::take(&mut debug.boxes);
    debug.boxes = std::mem::take(&mut debug.fixed);
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
        let page_box = paint::debug_page_box(spec);
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
