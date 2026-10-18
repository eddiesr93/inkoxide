//! The structure tree of tagged output: the elements of the plan
//! ([`crate::tag`]) that ended up with content, each with its marked
//! content (by MCID, on its pages) and its annotations (`OBJR`), the
//! parent tree that maps both back to their elements, and the elements
//! made here for annotations whose element holds content of another kind.

use std::collections::HashMap;
use std::rc::Rc;

use pdf_writer::{Dict, Finish, Name, Ref, Str};

use super::svg::{FormTags, Owner};
use super::{annot_rect, EncodedPage, Writer};
use crate::geom::Rect;
use crate::tag::{Elem, Kind, Plan, Scope, StructId};

/// What kind of annotation an annotation is, for the element it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnnotKind {
    Link,
    Widget,
    Other,
}

impl AnnotKind {
    /// The element an annotation of this kind is in.
    fn elem(self) -> Kind {
        match self {
            Self::Link => Kind::Link,
            Self::Widget => Kind::Form,
            Self::Other => Kind::Annot,
        }
    }
}

/// An annotation's `/StructParent`, when tagged output gave it a key.
pub(super) fn struct_parent(annot: &mut Dict, key: Option<i32>) {
    if let Some(key) = key {
        annot.pair(Name(b"StructParent"), key);
    }
}

/// One annotation of tagged output.
#[derive(Debug)]
pub(super) struct AnnotEntry {
    /// Its `/StructParent` key.
    pub key: i32,
    pub r: Ref,
    pub page: usize,
    /// The element its content is in; `None` in decoration.
    pub owner: Option<StructId>,
    pub kind: AnnotKind,
}

/// The form of an SVG placed with its parts tagged.
#[derive(Debug)]
pub(super) struct FormEntry {
    /// Its `/StructParents` key.
    pub key: i32,
    pub r: Ref,
    pub page: usize,
    /// The MCIDs of the page before it ([`super::Placement::after`]).
    pub after: i32,
    /// The element it is placed in.
    pub owner: StructId,
    /// Its content's structure, the same for every form of the SVG.
    pub tags: Rc<FormTags>,
}

/// The annotations and SVG forms written so far, and the next parent tree
/// key: those of the pages come first, one per page.
#[derive(Debug)]
pub(super) struct AnnotTags {
    next: i32,
    pub entries: Vec<AnnotEntry>,
    /// The forms of SVGs with their parts tagged, before the annotations.
    pub forms: Vec<FormEntry>,
    /// The elements links go to (structure destinations), numbered before
    /// the tree is written.
    dests: HashMap<StructId, Ref>,
}

impl AnnotTags {
    pub fn new(pages: usize) -> Self {
        Self {
            next: i32::try_from(pages).unwrap_or(i32::MAX),
            entries: Vec::new(),
            forms: Vec::new(),
            dests: HashMap::new(),
        }
    }

    /// The object of element `id`, which a link goes to: the tree writes
    /// it with this number, and keeps it.
    pub fn dest(&mut self, id: StructId, alloc: &mut super::RefAlloc) -> Ref {
        *self.dests.entry(id).or_insert_with(|| alloc.next())
    }

    /// The key the next annotation recorded gets.
    pub fn peek(&self) -> i32 {
        self.next
    }

    /// The `/StructParents` key of the next SVG form; forms are written
    /// before the annotations.
    pub fn form_key(&mut self) -> i32 {
        debug_assert!(self.entries.is_empty());
        let key = self.next;
        self.next = self.next.saturating_add(1);
        key
    }

    /// Records annotation `r` on page `page`; returns its `/StructParent`
    /// key.
    pub fn add(&mut self, r: Ref, page: usize, owner: Option<StructId>, kind: AnnotKind) -> i32 {
        let key = self.next;
        self.next = self.next.saturating_add(1);
        self.entries.push(AnnotEntry {
            key,
            r,
            page,
            owner,
            kind,
        });
        key
    }
}

/// A child of an element, in order.
#[derive(Debug, Clone, Copy)]
enum Kid {
    Elem(usize),
    /// A marked-content sequence: page and MCID.
    Mcid(usize, i32),
    /// An annotation: page and object.
    Obj(usize, Ref),
    /// A marked-content sequence of an SVG form: page, the page's MCIDs
    /// before the form, form and MCID.
    Stm(usize, i32, Ref, i32),
}

impl Kid {
    /// Where its content is among the page's sequences, when it is page
    /// content: a form's comes before the page's next sequence.
    fn at(self) -> Option<(usize, i32)> {
        match self {
            Self::Mcid(p, m) => Some((p, 2 * m + 1)),
            Self::Stm(p, after, ..) => Some((p, 2 * after)),
            Self::Elem(_) | Self::Obj(..) => None,
        }
    }
}

/// The written tree's elements: the plan's, then those made for
/// annotations.
struct Tree<'p> {
    elems: Vec<std::borrow::Cow<'p, Elem>>,
    kids: Vec<Vec<Kid>>,
}

/// Puts the elements inside an element that holds content (the links and
/// images of a paragraph) where their content is among its own: kids in
/// order of their first marked content, the rest after, as they were.
/// Elements of other kinds keep document order, whatever the order they
/// are drawn in.
fn interleave(kids: &mut [Vec<Kid>]) {
    // The first marked content of each element, at any depth: children
    // come after their parents.
    let mut first = vec![None; kids.len()];
    for id in (0..kids.len()).rev() {
        first[id] = kids[id]
            .iter()
            .filter_map(|k| match *k {
                Kid::Elem(c) => first[c],
                k => k.at(),
            })
            .min();
    }
    for k in kids.iter_mut() {
        if k.iter().any(|k| matches!(k, Kid::Mcid(..))) {
            k.sort_by_key(|k| {
                match *k {
                    Kid::Elem(c) => first[c],
                    k => k.at(),
                }
                .map_or((1, (0, 0)), |at| (0, at))
            });
        }
    }
}

/// Puts part `p` of a form, whose elements start at `base`, in the element
/// it is in, after the part around it, unless it is there.
fn place(tree: &mut Tree<'_>, tags: &FormTags, base: usize, p: usize, placed: &mut [bool]) {
    if std::mem::replace(&mut placed[p], true) {
        return;
    }
    if let Some(up) = tags.parts[p].1 {
        place(tree, tags, base, up, placed);
    }
    let parent = tree.elems[base + p].parent.unwrap_or(0) as usize;
    tree.kids[parent].push(Kid::Elem(base + p));
}

/// The standard structure namespaces of PDF 2.0 and of PDF 1.7 (ISO
/// 32000-2, 14.8.6).
const NS_PDF_2: &str = "http://iso.org/pdf2/ssn";
const NS_PDF_1_7: &str = "http://iso.org/pdf/ssn";

/// Writes the structure tree of `pages` (each with its tags) and `annots`
/// with root `root`. Elements without content are left out, unless a link
/// goes to them. For PDF 2.0 (`pdf_2`), every element is in a namespace:
/// PDF 2.0's, but for `BlockQuote`, which only PDF 1.7's has, mapped to
/// PDF 2.0's `Div` there.
pub(super) fn write_tree(
    w: &mut Writer,
    plan: &Plan,
    root: Ref,
    pages: &[EncodedPage],
    page_refs: &[Ref],
    annots: &AnnotTags,
    pdf_2: bool,
) {
    let mut tree = Tree {
        elems: plan.elems.iter().map(std::borrow::Cow::Borrowed).collect(),
        kids: vec![Vec::new(); plan.elems.len()],
    };
    // Content first, in page and MCID order.
    for (i, page) in pages.iter().enumerate() {
        let Some(mcids) = &page.mcids else { continue };
        for (mcid, &owner) in mcids.iter().enumerate() {
            tree.kids[owner as usize].push(Kid::Mcid(i, mcid as i32));
        }
    }
    // Annotations: in their element, or in one of their own under the
    // element whose content they are in (a link in a paragraph), or under
    // the document after its content (a link in a header), in page order.
    let mut elem_of_annot = Vec::with_capacity(annots.entries.len());
    let mut last = Vec::new();
    for a in &annots.entries {
        let kind = a.kind.elem();
        let into = match a.owner {
            Some(o) if tree.elems[o as usize].kind == kind => o as usize,
            owner => {
                let parent = owner.unwrap_or(0);
                tree.elems
                    .push(std::borrow::Cow::Owned(Elem::new(kind, Some(parent))));
                tree.kids.push(Vec::new());
                let id = tree.elems.len() - 1;
                if owner.is_some() {
                    tree.kids[parent as usize].push(Kid::Elem(id));
                } else {
                    last.push((a.page, Kid::Elem(id)));
                }
                id
            }
        };
        tree.kids[into].push(Kid::Obj(a.page, a.r));
        elem_of_annot.push(into);
    }
    // SVG forms: each named part an element in the one it is in, placed
    // there by its first content; the element of each MCID.
    let mut elem_of_mcid: Vec<Vec<usize>> = Vec::with_capacity(annots.forms.len());
    for f in &annots.forms {
        let base = tree.elems.len();
        for (alt, parent) in &f.tags.parts {
            let parent = parent.map_or(f.owner, |p| (base + p) as StructId);
            let mut e = Elem::new(Kind::Figure, Some(parent));
            e.alt = Some(alt.clone());
            tree.elems.push(std::borrow::Cow::Owned(e));
            tree.kids.push(Vec::new());
        }
        let mut placed = vec![false; f.tags.parts.len()];
        let mut elems = Vec::with_capacity(f.tags.mcids.len());
        for (mcid, owner) in f.tags.mcids.iter().enumerate() {
            let into = match *owner {
                Owner::Outer => f.owner as usize,
                Owner::Part(p) => {
                    place(&mut tree, &f.tags, base, p, &mut placed);
                    base + p
                }
            };
            tree.kids[into].push(Kid::Stm(f.page, f.after, f.r, mcid as i32));
            elems.push(into);
        }
        elem_of_mcid.push(elems);
    }
    // The plan's children, in plan order, after their parent's content.
    for (id, e) in plan.elems.iter().enumerate() {
        if let Some(p) = e.parent {
            tree.kids[p as usize].push(Kid::Elem(id));
        }
    }
    // Links, icons and widgets are recorded kind by kind: in page order.
    last.sort_by_key(|&(page, _)| page);
    tree.kids[0].extend(last.into_iter().map(|(_, k)| k));
    interleave(&mut tree.kids);
    // Which elements have content, at any depth: children come after
    // their parents.
    let mut kept = vec![false; tree.elems.len()];
    for id in (0..tree.elems.len()).rev() {
        kept[id] = tree.kids[id].iter().any(|k| match *k {
            Kid::Elem(c) => kept[c],
            _ => true,
        });
    }
    kept[0] = true;
    // What links go to, and the elements around it.
    for &id in annots.dests.keys() {
        let mut at = Some(id);
        while let Some(i) = at {
            kept[i as usize] = true;
            at = tree.elems[i as usize].parent;
        }
    }
    // A table keeps its empty cells: rows have the same number of columns
    // (ISO 14289-1, 7.2). Parents come before their children.
    for id in 0..tree.elems.len() {
        let e = &tree.elems[id];
        let part = matches!(
            e.kind,
            Kind::THead | Kind::TBody | Kind::TR | Kind::TH | Kind::TD
        );
        if part && e.parent.is_some_and(|p| kept[p as usize]) {
            kept[id] = true;
        }
    }
    let refs: Vec<Option<Ref>> = kept
        .iter()
        .enumerate()
        .map(|(id, &k)| {
            let dest = annots.dests.get(&(id as StructId)).copied();
            k.then(|| dest.unwrap_or_else(|| w.alloc.next()))
        })
        .collect();
    let parent_tree = w.alloc.next();
    let namespaces = pdf_2.then(|| {
        let quotes = tree
            .elems
            .iter()
            .zip(&kept)
            .any(|(e, &k)| k && e.kind == Kind::BlockQuote);
        (w.alloc.next(), quotes.then(|| w.alloc.next()))
    });

    // The root.
    let doc = refs[0].unwrap_or(root);
    let mut d = w.pdf.indirect(root).dict();
    d.pair(Name(b"Type"), Name(b"StructTreeRoot"))
        .pair(Name(b"K"), doc)
        .pair(Name(b"ParentTree"), parent_tree)
        .pair(Name(b"ParentTreeNextKey"), annots.peek());
    if let Some((ns2, ns17)) = namespaces {
        d.insert(Name(b"Namespaces"))
            .array()
            .items(std::iter::once(ns2).chain(ns17));
    }
    d.finish();
    if let Some((ns2, ns17)) = namespaces {
        let uri = w.out.text(NS_PDF_2);
        w.pdf
            .indirect(ns2)
            .dict()
            .pair(Name(b"Type"), Name(b"Namespace"))
            .pair(Name(b"NS"), Str(&uri));
        if let Some(ns17) = ns17 {
            let uri = w.out.text(NS_PDF_1_7);
            let mut d = w.pdf.indirect(ns17).dict();
            d.pair(Name(b"Type"), Name(b"Namespace"))
                .pair(Name(b"NS"), Str(&uri));
            d.insert(Name(b"RoleMapNS"))
                .dict()
                .insert(Name(Kind::BlockQuote.name()))
                .array()
                .item(Name(b"Div"))
                .item(ns2);
        }
    }

    // The parent tree: per page, the element of each MCID; per
    // annotation, its element.
    w.packable(parent_tree, |o| {
        let mut d = o.dict();
        let mut nums = d.insert(Name(b"Nums")).array();
        for (i, page) in pages.iter().enumerate() {
            nums.item(i as i32);
            let mut arr = nums.push().array();
            if let Some(mcids) = &page.mcids {
                // An MCID's place is its index: one without an element
                // keeps it, as null.
                for &owner in mcids {
                    match refs[owner as usize] {
                        Some(r) => arr.item(r),
                        None => arr.item(pdf_writer::Null),
                    };
                }
            }
            arr.finish();
        }
        for (f, elems) in annots.forms.iter().zip(&elem_of_mcid) {
            nums.item(f.key);
            nums.push()
                .array()
                .items(elems.iter().map(|&e| refs[e].unwrap_or(doc)));
        }
        for (a, &into) in annots.entries.iter().zip(&elem_of_annot) {
            nums.item(a.key);
            nums.item(refs[into].unwrap_or(doc));
        }
    });

    // Where each figure is, on each page it is on.
    let mut areas: HashMap<(StructId, usize), Rect> = HashMap::new();
    for (i, page) in pages.iter().enumerate() {
        for &(id, r) in &page.figures {
            areas
                .entry((id, i))
                .and_modify(|a| *a = a.union(r))
                .or_insert(r);
        }
    }

    // PDF 2.0: the first and the last child of each element whose
    // children are all kept elements, for the caption rule below.
    let ends: Vec<Option<(usize, usize)>> = if pdf_2 {
        tree.kids
            .iter()
            .map(|kids| {
                let all = kids.iter().all(|k| matches!(*k, Kid::Elem(c) if kept[c]));
                match (kids.first(), kids.last()) {
                    (Some(&Kid::Elem(a)), Some(&Kid::Elem(b))) if all => Some((a, b)),
                    _ => None,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // The elements.
    for (id, e) in tree.elems.iter().enumerate() {
        let Some(r) = refs[id] else { continue };
        let kids: Vec<Kid> = tree.kids[id]
            .iter()
            .copied()
            .filter(|k| !matches!(*k, Kid::Elem(c) if !kept[c]))
            .collect();
        // The page of its first content.
        let pg = kids.iter().find_map(|k| match *k {
            Kid::Mcid(p, _) | Kid::Obj(p, _) | Kid::Stm(p, ..) => Some(p),
            Kid::Elem(_) => None,
        });
        let alt = e.alt.as_deref().map(|a| w.packable_text(a));
        let lang = e.lang.as_deref().map(|l| w.packable_text(l));
        // The document's parent is the root; every other's, its element.
        let parent = match e.parent {
            None => root,
            Some(p) => refs[p as usize].unwrap_or(doc),
        };
        // PDF 2.0: a caption is the first or the last child of what it
        // captions, never of the document; another is a paragraph.
        let kind = match e.parent {
            Some(p) if pdf_2 && e.kind == Kind::Caption => {
                let edge = matches!(ends[p as usize], Some((a, b)) if a == id || b == id);
                if p == 0 || !edge {
                    Kind::P
                } else {
                    Kind::Caption
                }
            }
            _ => e.kind,
        };
        w.packable(r, |o| {
            let mut d = o.dict();
            d.pair(Name(b"Type"), Name(b"StructElem"));
            d.pair(Name(b"S"), Name(kind.name()));
            d.pair(Name(b"P"), parent);
            if let Some((ns2, ns17)) = namespaces {
                let ns = match ns17 {
                    Some(ns17) if e.kind == Kind::BlockQuote => ns17,
                    _ => ns2,
                };
                d.pair(Name(b"NS"), ns);
            }
            if let Some(p) = pg {
                d.pair(Name(b"Pg"), page_refs[p]);
            }
            if let Some(a) = &alt {
                d.pair(Name(b"Alt"), Str(a));
            }
            if let Some(l) = &lang {
                d.pair(Name(b"Lang"), Str(l));
            }
            // A figure's bounding box, on the page of its first content
            // (ISO 32000-1, 14.8.5.4.3).
            let area = pg.and_then(|p| Some((areas.get(&(id as StructId, p))?, pages[p].height)));
            if let Some((&r, page_h)) = area.filter(|_| e.kind == Kind::Figure) {
                let b = annot_rect(r, page_h);
                d.insert(Name(b"A"))
                    .dict()
                    .pair(Name(b"O"), Name(b"Layout"))
                    .insert(Name(b"BBox"))
                    .array()
                    .items([b.x1, b.y1, b.x2, b.y2]);
            }
            if let Some(n) = e.numbering {
                d.insert(Name(b"A"))
                    .dict()
                    .pair(Name(b"O"), Name(b"List"))
                    .pair(Name(b"ListNumbering"), Name(n.name()));
            }
            if e.scope.is_some() || e.row_span > 1 || e.col_span > 1 {
                let mut a = d.insert(Name(b"A")).dict();
                a.pair(Name(b"O"), Name(b"Table"));
                if let Some(s) = e.scope {
                    a.pair(
                        Name(b"Scope"),
                        Name(match s {
                            Scope::Row => b"Row",
                            Scope::Column => b"Column",
                        }),
                    );
                }
                if e.row_span > 1 {
                    a.pair(
                        Name(b"RowSpan"),
                        i32::try_from(e.row_span).unwrap_or(i32::MAX),
                    );
                }
                if e.col_span > 1 {
                    a.pair(
                        Name(b"ColSpan"),
                        i32::try_from(e.col_span).unwrap_or(i32::MAX),
                    );
                }
            }
            let mut k = d.insert(Name(b"K")).array();
            for kid in kids {
                match kid {
                    Kid::Elem(c) => {
                        if let Some(cr) = refs[c] {
                            k.item(cr);
                        }
                    }
                    Kid::Mcid(p, mcid) if Some(p) == pg => {
                        k.item(mcid);
                    }
                    Kid::Mcid(p, mcid) => {
                        k.push()
                            .dict()
                            .pair(Name(b"Type"), Name(b"MCR"))
                            .pair(Name(b"Pg"), page_refs[p])
                            .pair(Name(b"MCID"), mcid);
                    }
                    Kid::Obj(p, obj) => {
                        k.push()
                            .dict()
                            .pair(Name(b"Type"), Name(b"OBJR"))
                            .pair(Name(b"Pg"), page_refs[p])
                            .pair(Name(b"Obj"), obj);
                    }
                    Kid::Stm(p, _, form, mcid) => {
                        k.push()
                            .dict()
                            .pair(Name(b"Type"), Name(b"MCR"))
                            .pair(Name(b"Pg"), page_refs[p])
                            .pair(Name(b"Stm"), form)
                            .pair(Name(b"MCID"), mcid);
                    }
                }
            }
        });
    }
}
