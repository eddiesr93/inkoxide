//! Tagged PDF: the logical structure of a document, planned from the
//! document tree before layout, in document order. Painting marks what
//! each node's content draws with its element ([`crate::paint::Op::Tag`]);
//! everything else is decoration, which the writer marks as artifacts. The
//! reading order is therefore document order, whatever `zIndex`, floats or
//! positioning do on the page.
//!
//! Headers, footers, overlays and other nodes painted on every page are
//! decoration: they are not in the plan. So are nodes with the role
//! `artifact` and images with an empty `alt`.

use std::collections::HashMap;

use crate::schema::{Document, Node, NodeKind, Role, Table};

/// Index of an element in a [`Plan`].
pub(crate) type StructId = u32;

/// The structure types written: the standard ones of PDF 1.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Document,
    P,
    H(u8),
    Sect,
    BlockQuote,
    Caption,
    Figure,
    L,
    LI,
    Lbl,
    LBody,
    Table,
    THead,
    TBody,
    TR,
    TH,
    TD,
    Form,
    Annot,
    Link,
}

impl Kind {
    /// The element of `role`; `None` for decoration.
    pub fn of(role: Role) -> Option<Self> {
        Some(match role {
            Role::H1 => Self::H(1),
            Role::H2 => Self::H(2),
            Role::H3 => Self::H(3),
            Role::H4 => Self::H(4),
            Role::H5 => Self::H(5),
            Role::H6 => Self::H(6),
            Role::P => Self::P,
            Role::Section => Self::Sect,
            Role::BlockQuote => Self::BlockQuote,
            Role::Caption => Self::Caption,
            Role::Figure => Self::Figure,
            Role::List => Self::L,
            Role::ListItem => Self::LI,
            Role::Label => Self::Lbl,
            Role::ListBody => Self::LBody,
            Role::Artifact => return None,
        })
    }

    /// The structure type's name (`/S`), also the tag of its marked
    /// content.
    pub fn name(self) -> &'static [u8] {
        match self {
            Self::Document => b"Document",
            Self::P => b"P",
            Self::H(1) => b"H1",
            Self::H(2) => b"H2",
            Self::H(3) => b"H3",
            Self::H(4) => b"H4",
            Self::H(5) => b"H5",
            Self::H(_) => b"H6",
            Self::Sect => b"Sect",
            Self::BlockQuote => b"BlockQuote",
            Self::Caption => b"Caption",
            Self::Figure => b"Figure",
            Self::L => b"L",
            Self::LI => b"LI",
            Self::Lbl => b"Lbl",
            Self::LBody => b"LBody",
            Self::Table => b"Table",
            Self::THead => b"THead",
            Self::TBody => b"TBody",
            Self::TR => b"TR",
            Self::TH => b"TH",
            Self::TD => b"TD",
            Self::Form => b"Form",
            Self::Annot => b"Annot",
            Self::Link => b"Link",
        }
    }

    /// Whether what is inside it is its own content, without elements of
    /// its own: text in a figure or a paragraph box.
    pub fn holds_content(self) -> bool {
        matches!(
            self,
            Self::P | Self::H(_) | Self::Caption | Self::Figure | Self::Lbl
        )
    }
}

/// Which headers a `TH` is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Row,
    Column,
}

/// One structure element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Elem {
    pub kind: Kind,
    /// `None` for the root.
    pub parent: Option<StructId>,
    pub alt: Option<String>,
    /// Its language, when it differs from the document's.
    pub lang: Option<String>,
    pub scope: Option<Scope>,
    pub row_span: u32,
    pub col_span: u32,
    /// A list with labels: how they number its items.
    pub numbering: Option<Numbering>,
}

impl Elem {
    /// An element of `kind` under `parent` (`None` for the root), with no
    /// attributes.
    pub fn new(kind: Kind, parent: Option<StructId>) -> Self {
        Self {
            kind,
            parent,
            alt: None,
            lang: None,
            scope: None,
            row_span: 1,
            col_span: 1,
            numbering: None,
        }
    }
}

/// How the labels of a list number its items (ISO 32000-1, 14.8.5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Numbering {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    UpperRoman,
    LowerRoman,
    UpperAlpha,
    LowerAlpha,
}

impl Numbering {
    /// Its `/ListNumbering` name.
    pub fn name(self) -> &'static [u8] {
        match self {
            Self::None => b"None",
            Self::Disc => b"Disc",
            Self::Circle => b"Circle",
            Self::Square => b"Square",
            Self::Decimal => b"Decimal",
            Self::UpperRoman => b"UpperRoman",
            Self::LowerRoman => b"LowerRoman",
            Self::UpperAlpha => b"UpperAlpha",
            Self::LowerAlpha => b"LowerAlpha",
        }
    }

    /// What the labels of a list's items show, `labels` (their text, `1.`,
    /// `(a)`, `•`): `None` when they number no way PDF names, or there are
    /// none.
    pub fn of(labels: &[String]) -> Option<Self> {
        let marks: Vec<&str> = labels
            .iter()
            .map(|l| {
                l.trim()
                    .trim_start_matches('(')
                    .trim_end_matches(['.', ')', ':'])
            })
            .collect();
        if marks.is_empty() {
            return None;
        }
        let all = |f: &dyn Fn(&str) -> bool| marks.iter().all(|m| f(m));
        // One letter each, of a kind.
        let letters = |f: fn(&char) -> bool| -> Option<Vec<char>> {
            marks
                .iter()
                .map(|m| {
                    let mut c = m.chars();
                    c.next().filter(|ch| f(ch) && c.next().is_none())
                })
                .collect()
        };
        let consecutive = |c: &[char]| c.windows(2).all(|w| u32::from(w[1]) == u32::from(w[0]) + 1);
        let roman = |set: &str| all(&|m| !m.is_empty() && m.chars().all(|c| set.contains(c)));
        let n = if all(&|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit())) {
            Self::Decimal
        } else if all(&|m| matches!(m, "•" | "●" | "∙" | "⦁")) {
            Self::Disc
        } else if all(&|m| matches!(m, "◦" | "○")) {
            Self::Circle
        } else if all(&|m| matches!(m, "▪" | "■" | "▫" | "□")) {
            Self::Square
        } else if letters(char::is_ascii_lowercase).is_some_and(|c| consecutive(&c)) {
            Self::LowerAlpha
        } else if letters(char::is_ascii_uppercase).is_some_and(|c| consecutive(&c)) {
            Self::UpperAlpha
        } else if roman("ivxlcdm") {
            Self::LowerRoman
        } else if roman("IVXLCDM") {
            Self::UpperRoman
        } else if letters(char::is_ascii_lowercase).is_some() {
            Self::LowerAlpha
        } else if letters(char::is_ascii_uppercase).is_some() {
            Self::UpperAlpha
        } else {
            Self::None
        };
        Some(n)
    }
}

/// The text of the label of each item of list `list`: items are the
/// nodes with the role `listItem` inside it, through nodes without a
/// role, and an item's label the first node with the role `label` inside
/// it, the same way.
fn list_labels(list: &Node) -> Vec<String> {
    /// The nodes with a role inside `node`, through nodes without one, in
    /// document order.
    fn with_role(node: &Node) -> Vec<&Node> {
        let mut found = Vec::new();
        let mut stack: Vec<&Node> = node.shown_children().collect();
        stack.reverse();
        while let Some(n) = stack.pop() {
            if n.role.is_some() {
                found.push(n);
            } else {
                let first = stack.len();
                stack.extend(n.shown_children());
                stack[first..].reverse();
            }
        }
        found
    }
    with_role(list)
        .into_iter()
        .filter(|n| n.role == Some(Role::ListItem))
        .filter_map(|item| {
            let label = with_role(item)
                .into_iter()
                .find(|n| n.role == Some(Role::Label))?;
            let text = LinkText::of(label).finish().unwrap_or_default();
            Some(text.into())
        })
        .collect()
}

/// Most characters of a link's `/Contents` taken from the text it covers.
pub(crate) const MAX_LINK_TEXT: usize = 1024;

/// What a link says it is, gathered from the text it covers: pieces of
/// text with a space between them, at most [`MAX_LINK_TEXT`] characters.
#[derive(Debug, Default)]
pub(crate) struct LinkText {
    text: String,
    chars: usize,
}

impl LinkText {
    /// Adds `text`, as much of it as there is room for.
    pub fn push(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.is_full() {
            return;
        }
        if self.chars > 0 {
            self.text.push(' ');
            self.chars += 1;
        }
        for c in text.chars().take(MAX_LINK_TEXT - self.chars) {
            self.text.push(c);
            self.chars += 1;
        }
    }

    pub fn is_full(&self) -> bool {
        self.chars >= MAX_LINK_TEXT
    }

    /// The text; `None` when there is none.
    pub fn finish(self) -> Option<Box<str>> {
        (self.chars > 0).then(|| self.text.into())
    }

    /// What box `node` with a link says it is: the text of the runs and
    /// the alternative text of the images shown in it, in document order.
    pub fn of(node: &Node) -> Self {
        let mut out = Self::default();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if out.is_full() {
                break;
            }
            match &n.kind {
                NodeKind::Text { runs } => {
                    for t in runs.iter().filter_map(|r| r.text.as_deref()) {
                        out.push(t);
                    }
                }
                NodeKind::Image { alt: Some(alt), .. } => out.push(alt),
                _ => {}
            }
            let first = stack.len();
            stack.extend(n.shown_children());
            stack[first..].reverse();
        }
        out
    }
}

/// The structure elements of a document, and the element of each node
/// that draws content.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// Element 0 is the `Document`.
    pub elems: Vec<Elem>,
    /// Node (by address) → the element its content belongs to.
    content: HashMap<usize, StructId>,
    /// Node (by address) → the `Link` element of a box with a link.
    links: HashMap<usize, StructId>,
    /// Text node (by address) → the elements of its runs, when some run
    /// has one.
    runs: HashMap<usize, Box<[RunTag]>>,
    /// Node id → the element of the first node with that id, for the
    /// structure destinations of links to it.
    ids: HashMap<String, StructId>,
    /// Node with a bookmark (by address) → its element, for the
    /// structure destination of the bookmark.
    bookmarks: HashMap<usize, StructId>,
    /// The SVGs whose parts are tagged ([`crate::images::svg::SvgParts`]).
    pub parted: std::collections::BTreeSet<crate::images::ImageKey>,
    /// PDF 2.0 (PDF/UA-2): a box with a link holds its content in its
    /// `Link`, which may not hold a `P` there, and a block quotation's text
    /// is a `P` inside it, as PDF 1.7's `BlockQuote` maps to a `Div`,
    /// which holds no content.
    pdf_2: bool,
}

/// The elements of a run of text inside its paragraph's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RunTag {
    /// The element its content belongs to: its image's `Figure`, or its
    /// `Link`; `None`: the paragraph's.
    pub content: Option<StructId>,
    /// The `Link` element of a linked run, which holds its annotations.
    pub link: Option<StructId>,
}

/// A node's identity for the plan: its address, which every layout tree
/// borrows from the document.
fn key(node: &Node) -> usize {
    std::ptr::from_ref(node) as usize
}

impl Plan {
    /// The plan of `doc`; for PDF/UA-2 with `pdf_ua_2`.
    pub fn new(doc: &Document, pdf_ua_2: bool) -> Self {
        let mut plan = Self {
            elems: vec![Elem::new(Kind::Document, None)],
            pdf_2: pdf_ua_2,
            ..Self::default()
        };
        let lang = doc.meta.lang.as_deref();
        for page in &doc.pages {
            for node in &page.children {
                plan.node(node, 0, None, lang);
            }
        }
        plan
    }

    /// With the parts of `images`' SVGs tagged.
    pub fn with_parted(mut self, images: &crate::images::ImageSet) -> Self {
        self.parted = images.parted();
        self
    }

    /// The element the content of `node` belongs to; `None` for
    /// decoration.
    pub fn owner(&self, node: &Node) -> Option<StructId> {
        self.content.get(&key(node)).copied()
    }

    /// The element of the first node with id `id`.
    pub fn anchor(&self, id: &str) -> Option<StructId> {
        self.ids.get(id).copied()
    }

    /// The element of `node`, which has a bookmark.
    pub fn bookmark(&self, node: &Node) -> Option<StructId> {
        self.bookmarks.get(&key(node)).copied()
    }

    /// The `Link` element of a box with a link.
    pub fn link(&self, node: &Node) -> Option<StructId> {
        self.links.get(&key(node)).copied()
    }

    /// The elements of the runs of text node `node`, when some run has
    /// one.
    pub fn runs(&self, node: &Node) -> Option<&[RunTag]> {
        self.runs.get(&key(node)).map(|r| &**r)
    }

    /// Plans the runs of a text whose content belongs to element `id`: a
    /// `Link` for each stretch of runs with one target, holding their
    /// text and annotations, and a `Figure` for each image with
    /// alternative text (ISO 14289-1, 7.3 and 7.18.5). A figure's
    /// alternative text says all of it: nothing inside one gets an element.
    fn runs_of(&mut self, node: &Node, runs: &[crate::schema::TextRun], id: StructId) {
        if self.elems[id as usize].kind == Kind::Figure {
            return;
        }
        let mut tags = vec![RunTag::default(); runs.len()];
        let mut link: Option<(&str, StructId)> = None;
        for (r, tag) in runs.iter().zip(&mut tags) {
            link = r.link.as_deref().map(|t| match link {
                Some((prev, l)) if prev == t => (t, l),
                _ => (t, self.push(Elem::new(Kind::Link, Some(id)))),
            });
            tag.link = link.map(|(_, l)| l);
            tag.content = tag.link;
            if let Some(alt) = r.image.as_ref().and_then(|im| im.alt.as_deref()) {
                if !alt.is_empty() {
                    let mut e = Elem::new(Kind::Figure, Some(tag.link.unwrap_or(id)));
                    e.alt = Some(alt.to_string());
                    tag.content = Some(self.push(e));
                }
            }
        }
        if tags.iter().any(|t| t.content.is_some()) {
            self.runs.insert(key(node), tags.into());
        }
    }

    fn push(&mut self, elem: Elem) -> StructId {
        self.elems.push(elem);
        (self.elems.len() - 1) as StructId
    }

    /// Plans `node` under element `parent`; inside an element that holds
    /// its content (`merge`), its content is that element's.
    fn node(&mut self, node: &Node, parent: StructId, merge: Option<StructId>, lang: Option<&str>) {
        if node.is_decoration() {
            return;
        }
        let own = self.node_elems(node, parent, merge, lang);
        if let Some(id) = &node.id {
            self.ids.entry(id.clone()).or_insert(own);
        }
        if node.bookmark.is_some() {
            self.bookmarks.insert(key(node), own);
        }
    }

    /// Plans `node`, not decoration ([`node`](Self::node)); returns its
    /// element: its own, or the one its content is in.
    fn node_elems(
        &mut self,
        node: &Node,
        parent: StructId,
        merge: Option<StructId>,
        lang: Option<&str>,
    ) -> StructId {
        let own_lang = node.style.lang.as_deref().filter(|l| Some(*l) != lang);
        match &node.kind {
            NodeKind::Text { runs } => {
                let id = merge.unwrap_or_else(|| {
                    let kind = node.role.and_then(Kind::of).unwrap_or(Kind::P);
                    let mut e = Elem::new(kind, Some(parent));
                    e.lang = own_lang.map(str::to_string);
                    let id = self.push(e);
                    if kind == Kind::BlockQuote && self.pdf_2 {
                        self.push(Elem::new(Kind::P, Some(id)))
                    } else {
                        id
                    }
                });
                self.content.insert(key(node), id);
                self.runs_of(node, runs, id);
                id
            }
            NodeKind::Image { alt, .. } => {
                if alt.as_deref() == Some("") {
                    return merge.unwrap_or(parent);
                }
                let id = merge.unwrap_or_else(|| {
                    let mut e = Elem::new(Kind::Figure, Some(parent));
                    e.alt.clone_from(alt);
                    self.push(e)
                });
                self.content.insert(key(node), id);
                id
            }
            NodeKind::Box { children, alt } => {
                let mut parent = parent;
                let mut merge = merge;
                // Its role's element, else its link's.
                let mut own = None;
                let around = merge.unwrap_or(parent);
                if merge.is_none() {
                    if let Some(kind) = node.role.and_then(Kind::of) {
                        let mut e = Elem::new(kind, Some(parent));
                        e.alt.clone_from(alt);
                        e.lang = own_lang.map(str::to_string);
                        if kind == Kind::L {
                            e.numbering = Numbering::of(&list_labels(node));
                        }
                        parent = self.push(e);
                        own = Some(parent);
                        if kind.holds_content() {
                            merge = Some(parent);
                        }
                        // A figure's own background and borders are its
                        // content: it may draw nothing else.
                        if kind == Kind::Figure {
                            self.content.insert(key(node), parent);
                        }
                    }
                    if node.link.is_some() {
                        let link = self.push(Elem::new(Kind::Link, Some(parent)));
                        self.links.insert(key(node), link);
                        own.get_or_insert(link);
                        parent = link;
                        if self.pdf_2 {
                            merge = Some(link);
                        }
                    }
                } else if node.link.is_some() {
                    // A link inside a paragraph: its annotation joins the
                    // paragraph's.
                    self.links.insert(key(node), merge.unwrap_or(parent));
                }
                for c in children {
                    self.node(c, parent, merge, own_lang.or(lang));
                }
                own.unwrap_or(around)
            }
            NodeKind::Table(t) => {
                if let Some(m) = merge {
                    for c in node.children() {
                        self.node(c, m, Some(m), lang);
                    }
                    m
                } else {
                    self.table(t, parent, lang)
                }
            }
            NodeKind::Field(_) => {
                let id = self.push(Elem::new(Kind::Form, Some(merge.unwrap_or(parent))));
                self.content.insert(key(node), id);
                id
            }
            NodeKind::Note(_) | NodeKind::Attachment(_) => {
                let id = self.push(Elem::new(Kind::Annot, Some(merge.unwrap_or(parent))));
                self.content.insert(key(node), id);
                id
            }
            NodeKind::PageBreak | NodeKind::Spacer => merge.unwrap_or(parent),
        }
    }

    /// A table: its header rows in `THead`, the others in `TBody`; the
    /// cells of header rows are `TH` for their column, the other cells
    /// `TD`, or `TH` for their row with `header`. Returns the `Table`.
    fn table(&mut self, t: &Table, parent: StructId, lang: Option<&str>) -> StructId {
        let table = self.push(Elem::new(Kind::Table, Some(parent)));
        let heads = (t.header_rows as usize).min(t.rows.len());
        let (head, body) = t.rows.split_at(heads);
        for (rows, kind) in [(head, Kind::THead), (body, Kind::TBody)] {
            if rows.is_empty() {
                continue;
            }
            let group = self.push(Elem::new(kind, Some(table)));
            for row in rows {
                let tr = self.push(Elem::new(Kind::TR, Some(group)));
                for cell in &row.cells {
                    let header = kind == Kind::THead || cell.header;
                    let mut e = Elem::new(if header { Kind::TH } else { Kind::TD }, Some(tr));
                    if header {
                        e.scope = Some(if kind == Kind::THead {
                            Scope::Column
                        } else {
                            Scope::Row
                        });
                    }
                    e.row_span = cell.row_span;
                    e.col_span = cell.col_span;
                    let td = self.push(e);
                    for c in &cell.children {
                        self.node(c, td, None, lang);
                    }
                }
            }
        }
        table
    }
}

#[cfg(test)]
mod tests {
    use super::Numbering;

    #[test]
    fn labels_say_how_a_list_is_numbered() {
        let of = |labels: &[&str]| {
            Numbering::of(&labels.iter().map(|l| (*l).to_string()).collect::<Vec<_>>())
        };
        assert_eq!(of(&[]), None);
        for (labels, n) in [
            (&["1.", "2.", "10."][..], Numbering::Decimal),
            (&["(1)", "(2)"], Numbering::Decimal),
            (&["•", "•"], Numbering::Disc),
            (&["◦"], Numbering::Circle),
            (&["▪", "■"], Numbering::Square),
            (&["a)", "b)", "c)"], Numbering::LowerAlpha),
            (&["A.", "B."], Numbering::UpperAlpha),
            (&["i.", "ii.", "iii.", "iv."], Numbering::LowerRoman),
            (&["I", "II"], Numbering::UpperRoman),
            // Consecutive letters count; others may be Roman numerals.
            (&["c", "d"], Numbering::LowerAlpha),
            (&["v", "x"], Numbering::LowerRoman),
            (&["–", "–"], Numbering::None),
            (&["1.", "b."], Numbering::None),
        ] {
            assert_eq!(of(labels), Some(n), "{labels:?}");
        }
    }
}
