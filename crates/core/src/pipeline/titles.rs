//! Bookmark titles in headers, footers and overlays: what each paragraph
//! that shows them reads as on each page.

use std::collections::BTreeMap;

use super::{collect_texts, Laid, Marks};
use crate::schema::{Node, NodeKind};
use crate::text::TitledText;

/// The levels of the bookmarks whose titles the runs of text `node` show,
/// one per `bookmarkTitle` run, in order.
fn levels(node: &Node) -> Vec<u8> {
    match &node.kind {
        NodeKind::Text { runs } => runs.iter().filter_map(|r| r.bookmark_title).collect(),
        _ => Vec::new(),
    }
}

/// The paragraphs of headers, footers and overlays that show bookmark
/// titles, as they read on each page. A page shows, for a level, the
/// title of the last bookmark of that level that starts on it or before
/// it, unless a bookmark of a higher level (a smaller number) starts after
/// that one: a new chapter has no section until its first one. Before
/// any, the run is empty, and the paragraph is the node itself.
pub(super) fn titled_text<'d>(laid: &[Laid<'d>], marks: &Marks<'d>) -> TitledText {
    let mut out = TitledText::default();
    // Per page template: its paragraphs that show titles, and their levels.
    let wanted: Vec<Vec<(&Node, Vec<u8>)>> = laid
        .iter()
        .map(|l| {
            let bands = l.header.iter().chain(&l.footer);
            let bands = bands.chain(l.overlays.iter().map(|(_, b)| b));
            let mut texts = Vec::new();
            for b in bands {
                collect_texts(b.node, &mut texts);
            }
            let texts = texts.into_iter().map(|n| (n, levels(n)));
            texts.filter(|(_, levels)| !levels.is_empty()).collect()
        })
        .collect();
    if wanted.iter().all(Vec::is_empty) {
        return out;
    }
    // The bookmarks by page, in document order on each.
    let mut entries: Vec<_> = marks.outline.iter().collect();
    entries.sort_by_key(|e| e.dest.page);
    let mut entries = entries.into_iter().peekable();
    let mut current: BTreeMap<u8, &str> = BTreeMap::new();
    let mut page = 0;
    for (l, texts) in laid.iter().zip(&wanted) {
        for _ in 0..l.placement.pages {
            while let Some(e) = entries.next_if(|e| e.dest.page <= page) {
                current.split_off(&e.level);
                current.insert(e.level, e.title);
            }
            for (node, levels) in texts {
                let titles: Vec<&str> = levels
                    .iter()
                    .map(|level| current.get(level).copied().unwrap_or(""))
                    .collect();
                if titles.iter().any(|t| !t.is_empty()) {
                    let copy = out.copy(node, &titles);
                    out.show(node, page, copy);
                }
            }
            page += 1;
        }
    }
    out
}
