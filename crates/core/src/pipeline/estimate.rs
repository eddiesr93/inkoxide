//! The bound on content repeated on every page (headers, footers,
//! overlays, debug outlines), estimated before anything is painted.

use super::{Cache, Laid};
use crate::layout::{NodeIdx, Part, Tree};
use crate::schema::{Decoration, DecorationStyle, NodeKind, PageFilter, PagePattern, TextRun};
use crate::text::WAVES_PER_GLYPH;
use crate::InkoxideError;

/// Most bytes the content painted on every page (headers, footers,
/// overlays, fixed nodes, repeated table header rows) may add to one
/// document, estimated before anything is painted: a memory guard like
/// [`MAX_PAGES`], since a few kilobytes repeated on every page of a long
/// document would otherwise make gigabytes.
pub(crate) const MAX_REPEATED: u64 = 64 << 20;

/// Estimated output, in bytes per page, of what a repeated node paints: a
/// node, or a character or glyph of text,
pub(super) const ITEM_BYTES: u64 = 16;

/// a link (a linked run at worst puts one on every line, a line per
/// character) or an image in a span with its text (an inline image, a
/// colour glyph or an image node with `alt`),
pub(super) const LINK_BYTES: u64 = 256;

/// a `debug` outline, a form field widget or a background image,
pub(super) const BOX_BYTES: u64 = 768;

/// and a shape of a paragraph's `debug` outline: a line box, a baseline or
/// a run's box on a line.
pub(super) const LINE_SHAPE_BYTES: u64 = 64;

/// Estimated output of the scripts of a field format, in bytes.
pub(super) const SCRIPT_BYTES: u64 = 512;

/// Estimated output, in bytes per page, of the `wavy` decoration under a
/// glyph: at most [`WAVES_PER_GLYPH`] half waves, a curve each.
pub(super) const WAVE_BYTES: u64 = WAVES_PER_GLYPH as u64 * 48;

/// Checks that the content painted on every page stays within
/// [`MAX_REPEATED`].
pub(super) fn repeated_size(
    laid: &[Laid<'_>],
    text: &Cache<'_, '_, '_>,
) -> Result<(), InkoxideError> {
    let mut total: u64 = 0;
    for l in laid {
        let (all, pages) = (l.spec.debug, u64::from(l.placement.pages));
        let bands = l.header.iter().chain(&l.footer);
        let bands = bands.chain(l.overlays.iter().map(|(_, b)| b));
        for b in bands {
            total = total.saturating_add(repeated(&b.tree, 0, pages, all, text));
        }
        for &i in &l.placement.fixed {
            total = total.saturating_add(repeated(&l.flow, i, pages, all, text));
        }
        // Header rows, again on every page after a table's first.
        for (&t, table) in &l.flow.tables {
            let (first, last) = l.placement.page_range[t as usize];
            for r in l.flow.children(t).take(table.header_rows) {
                let size = repeated(&l.flow, r, u64::from(last - first), all, text);
                total = total.saturating_add(size);
            }
        }
    }
    if total > MAX_REPEATED {
        return Err(InkoxideError::Layout(format!(
            "the headers, footers and fixed elements repeated on every page add about {} MB; \
             the limit is {} MB",
            total >> 20,
            MAX_REPEATED >> 20
        )));
    }
    Ok(())
}

/// Estimated output, in bytes, of node `i` of `tree` and its descendants
/// painted on `pages` pages (fewer for nodes shown on some pages only);
/// `all`: every box has a `debug` outline.
pub(super) fn repeated(
    tree: &Tree<'_>,
    i: NodeIdx,
    pages: u64,
    all: bool,
    text: &Cache<'_, '_, '_>,
) -> u64 {
    let mut total: u64 = 0;
    let mut stack = vec![(i, pages)];
    while let Some((j, pages)) = stack.pop() {
        let ln = &tree.nodes[j as usize];
        let debug = all
            || match ln.part {
                Part::Node(n) => n.debug,
                Part::Row(r) => r.debug,
                Part::Cell(c) => c.debug,
                Part::Root => false,
            };
        let mut pages = pages;
        let mut size = if debug { BOX_BYTES } else { ITEM_BYTES };
        if let Some(n) = ln.node() {
            if let Some(most) = n.pages.as_deref().and_then(most_pages) {
                pages = pages.min(most);
            }
            if let Some(target) = &n.link {
                size += LINK_BYTES + crate::pdf::written_uri_len(target) as u64;
            }
            if n.background_image.is_some() {
                size += BOX_BYTES;
            }
            match &n.kind {
                NodeKind::Text { runs } => {
                    let mut bytes: u64 = 0;
                    let mut wavy = false;
                    // A link per piece of a run on a line, at most: one per
                    // segment, plus one per line where a segment wraps or a
                    // hyphen or an ellipsis shows.
                    let mut pieces = None;
                    for r in runs {
                        if let Some(note) = &r.note {
                            size = size.saturating_add(note_bytes(note));
                        }
                        if let Some(a) = &r.attachment {
                            size = size.saturating_add(attachment_bytes(a));
                        }
                        // A placeholder shows a page number.
                        let chars = r.text.as_ref().map_or(8, String::len) as u64;
                        let mut each = r.link.as_ref().map_or(ITEM_BYTES, |_| LINK_BYTES);
                        if is_wavy(r) {
                            wavy = true;
                            each += WAVE_BYTES;
                        }
                        size = size.saturating_add(chars * each);
                        bytes += chars;
                        if let Some(target) = &r.link {
                            let most = *pieces.get_or_insert_with(|| {
                                text.paragraph(n, &tree.line_boxes(j))
                                    .map_or(u64::MAX, |(l, p, _)| {
                                        (p.segments.len() + 2 * l.lines.len()) as u64
                                    })
                            });
                            let uri = crate::pdf::written_uri_len(target) as u64;
                            size = size.saturating_add(chars.min(most).saturating_mul(uri));
                        }
                    }
                    // Glyphs beyond one per byte: a font's layout tables
                    // can multiply them.
                    let glyphs = text.glyph_count(n).unwrap_or(0) as u64;
                    let each = if wavy {
                        ITEM_BYTES + WAVE_BYTES
                    } else {
                        ITEM_BYTES
                    };
                    size = size.saturating_add(glyphs.saturating_sub(bytes) * each);
                    // Images in the text, each in a span with its text.
                    let colored = text.image_glyph_count(n) as u64;
                    size = size.saturating_add(colored * LINK_BYTES);
                    size = size.saturating_add(
                        text.image_texts(n)
                            .map(|t| crate::pdf::written_text_len(t) as u64)
                            .sum(),
                    );
                    if debug {
                        // A box and a baseline per line, and a box per
                        // stretch of a run on a line: at most one per
                        // segment, plus one per line where a segment wraps
                        // or a hyphen shows.
                        if let Some((l, p, _)) = text.paragraph(n, &tree.line_boxes(j)) {
                            let shapes = 4 * l.lines.len() + p.segments.len();
                            size = size.saturating_add(shapes as u64 * LINE_SHAPE_BYTES);
                        }
                    }
                }
                NodeKind::Field(f) => {
                    // Its options and values, in the field and in its
                    // appearance.
                    let text: usize = f.options.iter().chain(f.values()).map(String::len).sum();
                    size = size.saturating_add(BOX_BYTES + text as u64 * ITEM_BYTES);
                    if f.format.is_some() {
                        // Its two scripts.
                        size = size.saturating_add(SCRIPT_BYTES);
                    }
                }
                NodeKind::Note(note) => size = size.saturating_add(note_bytes(note)),
                NodeKind::Attachment(a) => size = size.saturating_add(attachment_bytes(a)),
                // In a span with its text.
                NodeKind::Image { alt: Some(alt), .. } => {
                    let text = crate::pdf::written_text_len(alt) as u64;
                    size = size.saturating_add(LINK_BYTES + text);
                }
                _ => {}
            }
        }
        total = total.saturating_add(size.saturating_mul(pages));
        stack.extend(tree.children(j).map(|c| (c, pages)));
    }
    total
}

/// Whether run `r` has a `wavy` decoration.
pub(super) fn is_wavy(r: &TextRun) -> bool {
    r.decoration.is_some_and(|d| d != Decoration::None)
        && r.decoration_style == Some(DecorationStyle::Wavy)
}

/// Estimated output of a note, in bytes: its icon, and its text as the
/// file holds it.
pub(super) fn note_bytes(note: &crate::schema::Note) -> u64 {
    let text = crate::pdf::written_text_len(&note.contents)
        + note
            .author
            .as_deref()
            .map_or(0, crate::pdf::written_text_len);
    BOX_BYTES + text as u64
}

/// Estimated output of an attachment's icon, in bytes: its icon, and its
/// text as the file holds it (its file's description or name at most a
/// note's length).
pub(super) fn attachment_bytes(a: &crate::schema::Attachment) -> u64 {
    // A description: at most `MAX_NOTE` characters, each two UTF-16 units
    // at most, in hex (see `written_text_len`).
    const DESCRIPTION: usize = 2 + 2 * (2 + 4 * crate::validate::rules::MAX_NOTE + 32);
    let text = a
        .contents
        .as_deref()
        .map_or(DESCRIPTION, crate::pdf::written_text_len);
    BOX_BYTES + text as u64
}

/// Most pages a node with page filter `f` is painted on, when that is a
/// fixed number.
pub(super) fn most_pages(f: &PageFilter) -> Option<u64> {
    match f {
        PageFilter::Pattern(PagePattern::First | PagePattern::Last) => Some(1),
        PageFilter::List(list) => Some(list.len() as u64),
        PageFilter::Pattern(_) => None,
    }
}
