//! Navigation: link annotations and the document outline.

use pdf_writer::types::{ActionType, AnnotationFlags, AnnotationType};
use pdf_writer::{Name, Rect as PdfRect, Ref, Str};

use crate::geom::Rect;
use crate::pipeline::{Dest, Marks};

use super::{round_pt, Profile, Writer};

/// Writes one link annotation as object `r`; `false` (nothing written)
/// when an anchor has no destination.
pub(super) fn write_link(
    w: &mut Writer,
    r: Ref,
    page_h: f32,
    (rect, target): &(Rect, Box<str>),
    (marks, pages): (&Marks<'_>, &[Ref]),
    profile: Profile,
) -> bool {
    let dest = match target.strip_prefix('#') {
        Some(id) => match marks.anchors.get(id) {
            Some(d) => Some(*d),
            None => return false,
        },
        None => None,
    };
    let uri = dest.is_none().then(|| w.out.bytes(&uri_bytes(target)));
    let mut a = w.pdf.annotation(r);
    a.subtype(AnnotationType::Link)
        .rect(annot_rect(*rect, page_h))
        .border(0.0, 0.0, 0.0, None);
    if profile.is_pdf_a() {
        a.flags(AnnotationFlags::PRINT);
    }
    let mut action = a.action();
    match dest {
        Some(d) => {
            action.action_type(ActionType::GoTo);
            xyz(action.destination(), d, pages);
        }
        None => {
            action
                .action_type(ActionType::Uri)
                .uri(Str(uri.as_deref().unwrap_or_default()));
        }
    }
    true
}

/// An annotation's `/Rect`: `r` (page coordinates, top-left origin) in
/// PDF user space, whose origin is the bottom left of a page `page_h` high.
pub(super) fn annot_rect(r: Rect, page_h: f32) -> PdfRect {
    PdfRect::new(
        round_pt(r.x),
        round_pt(page_h - r.y - r.h),
        round_pt(r.x + r.w),
        round_pt(page_h - r.y),
    )
}

/// `[page /XYZ left top 0]`: keeps the viewer's zoom.
pub(super) fn xyz(d: pdf_writer::writers::Destination<'_>, dest: Dest, pages: &[Ref]) {
    d.page(pages[dest.page as usize])
        .xyz(round_pt(dest.x), round_pt(dest.y), None);
}

/// Most bytes URI `uri` takes in the file once written by [`write_link`],
/// delimiters included: percent-encoded, at most doubled by escapes, and
/// encrypted with up to 32 bytes of padding and IV first.
pub(crate) fn written_uri_len(uri: &str) -> usize {
    let bytes: usize = uri
        .bytes()
        .map(|b| if b.is_ascii_graphic() { 1 } else { 3 })
        .sum();
    2 + 2 * (bytes + 32)
}

/// PDF URIs are 7-bit ASCII: percent-encode everything else (and spaces).
pub(super) fn uri_bytes(uri: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(uri.len());
    for &b in uri.as_bytes() {
        if b.is_ascii_graphic() {
            out.push(b);
        } else {
            out.extend_from_slice(format!("%{b:02X}").as_bytes());
        }
    }
    out
}

/// Writes the document outline; returns its root. Each entry nests under
/// the nearest earlier entry of a lower level; all entries start open.
pub(super) fn write_outline(
    w: &mut Writer,
    entries: &[crate::pipeline::OutlineEntry<'_>],
    page_refs: &[Ref],
) -> Option<Ref> {
    if entries.is_empty() {
        return None;
    }
    let Writer { pdf, alloc, out } = w;
    let root = alloc.next();
    let refs: Vec<Ref> = entries.iter().map(|_| alloc.next()).collect();
    // parent[i]: index of the parent entry (None: the outline root).
    let n = entries.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut stack: Vec<usize> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        while stack.last().is_some_and(|&j| entries[j].level >= e.level) {
            stack.pop();
        }
        parent[i] = stack.last().copied();
        stack.push(i);
    }
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut top = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) => kids[*p].push(i),
            None => top.push(i),
        }
    }
    // Descendants of each entry (entries are in pre-order).
    let mut count = vec![0i32; n];
    for i in (0..n).rev() {
        if let Some(p) = parent[i] {
            count[p] += count[i] + 1;
        }
    }
    let (mut prev, mut next) = (vec![None; n], vec![None; n]);
    for sib in kids.iter().chain([&top]) {
        for w in sib.windows(2) {
            next[w[0]] = Some(w[1]);
            prev[w[1]] = Some(w[0]);
        }
    }
    for (i, e) in entries.iter().enumerate() {
        let title = out.text(e.title);
        let mut item = pdf.outline_item(refs[i]);
        item.pair(Name(b"Title"), Str(&title));
        item.parent(parent[i].map_or(root, |p| refs[p]));
        if let Some(p) = prev[i] {
            item.prev(refs[p]);
        }
        if let Some(x) = next[i] {
            item.next(refs[x]);
        }
        if let (Some(&f), Some(&l)) = (kids[i].first(), kids[i].last()) {
            item.first(refs[f]).last(refs[l]).count(count[i]);
        }
        xyz(item.dest(), e.dest, page_refs);
    }
    let mut o = pdf.outline(root);
    if let (Some(&f), Some(&l)) = (top.first(), top.last()) {
        o.first(refs[f]).last(refs[l]);
    }
    o.count(n as i32);
    Some(root)
}
