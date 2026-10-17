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
    fn holds_content(self) -> bool {
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
}

impl Elem {
    fn new(kind: Kind, parent: StructId) -> Self {
        Self {
            kind,
            parent: Some(parent),
            alt: None,
            lang: None,
            scope: None,
            row_span: 1,
            col_span: 1,
        }
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
}

/// A node's identity for the plan: its address, which every layout tree
/// borrows from the document.
fn key(node: &Node) -> usize {
    std::ptr::from_ref(node) as usize
}

impl Plan {
    /// The plan of `doc`.
    pub fn new(doc: &Document) -> Self {
        let mut plan = Self {
            elems: vec![Elem {
                kind: Kind::Document,
                parent: None,
                alt: None,
                lang: None,
                scope: None,
                row_span: 1,
                col_span: 1,
            }],
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

    /// The element the content of `node` belongs to; `None` for
    /// decoration.
    pub fn owner(&self, node: &Node) -> Option<StructId> {
        self.content.get(&key(node)).copied()
    }

    /// The `Link` element of a box with a link.
    pub fn link(&self, node: &Node) -> Option<StructId> {
        self.links.get(&key(node)).copied()
    }

    fn push(&mut self, elem: Elem) -> StructId {
        self.elems.push(elem);
        (self.elems.len() - 1) as StructId
    }

    /// Plans `node` under element `parent`; inside an element that holds
    /// its content (`merge`), its content is that element's.
    fn node(&mut self, node: &Node, parent: StructId, merge: Option<StructId>, lang: Option<&str>) {
        // Painted on every page, or decoration.
        if node.brk.fixed || node.role == Some(Role::Artifact) {
            return;
        }
        let own_lang = node.style.lang.as_deref().filter(|l| Some(*l) != lang);
        match &node.kind {
            NodeKind::Text { .. } => {
                let id = merge.unwrap_or_else(|| {
                    let kind = match node.role {
                        Some(r) => r.heading().map_or_else(|| role_kind(r), Kind::H),
                        None => Kind::P,
                    };
                    let mut e = Elem::new(kind, parent);
                    e.lang = own_lang.map(str::to_string);
                    self.push(e)
                });
                self.content.insert(key(node), id);
            }
            NodeKind::Image { alt, .. } => {
                if alt.as_deref() == Some("") {
                    return;
                }
                let id = merge.unwrap_or_else(|| {
                    let mut e = Elem::new(Kind::Figure, parent);
                    e.alt.clone_from(alt);
                    self.push(e)
                });
                self.content.insert(key(node), id);
            }
            NodeKind::Box { children, alt } => {
                let mut parent = parent;
                let mut merge = merge;
                if merge.is_none() {
                    if let Some(role) = node.role {
                        let kind = role.heading().map_or_else(|| role_kind(role), Kind::H);
                        let mut e = Elem::new(kind, parent);
                        e.alt.clone_from(alt);
                        e.lang = own_lang.map(str::to_string);
                        parent = self.push(e);
                        if kind.holds_content() {
                            merge = Some(parent);
                        }
                    }
                    if node.link.is_some() {
                        let link = self.push(Elem::new(Kind::Link, parent));
                        self.links.insert(key(node), link);
                        parent = link;
                    }
                } else if node.link.is_some() {
                    // A link inside a paragraph: its annotation joins the
                    // paragraph's.
                    self.links.insert(key(node), merge.unwrap_or(parent));
                }
                for c in children {
                    self.node(c, parent, merge, own_lang.or(lang));
                }
            }
            NodeKind::Table(t) => {
                if let Some(m) = merge {
                    for c in node.children() {
                        self.node(c, m, Some(m), lang);
                    }
                } else {
                    self.table(t, parent, lang);
                }
            }
            NodeKind::Field(_) => {
                let id = self.push(Elem::new(Kind::Form, merge.unwrap_or(parent)));
                self.content.insert(key(node), id);
            }
            NodeKind::Note(_) | NodeKind::Attachment(_) => {
                let id = self.push(Elem::new(Kind::Annot, merge.unwrap_or(parent)));
                self.content.insert(key(node), id);
            }
            NodeKind::PageBreak | NodeKind::Spacer => {}
        }
    }

    /// A table: its header rows in `THead`, the others in `TBody`; the
    /// cells of header rows are `TH` for their column, the other cells
    /// `TD`, or `TH` for their row with `header`.
    fn table(&mut self, t: &Table, parent: StructId, lang: Option<&str>) {
        let table = self.push(Elem::new(Kind::Table, parent));
        let heads = (t.header_rows as usize).min(t.rows.len());
        let (head, body) = t.rows.split_at(heads);
        for (rows, kind) in [(head, Kind::THead), (body, Kind::TBody)] {
            if rows.is_empty() {
                continue;
            }
            let group = self.push(Elem::new(kind, table));
            for row in rows {
                let tr = self.push(Elem::new(Kind::TR, group));
                for cell in &row.cells {
                    let header = kind == Kind::THead || cell.header;
                    let mut e = Elem::new(if header { Kind::TH } else { Kind::TD }, tr);
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
    }
}

/// The element of a role that is not a heading.
fn role_kind(role: Role) -> Kind {
    match role {
        Role::Section => Kind::Sect,
        Role::BlockQuote => Kind::BlockQuote,
        Role::Caption => Kind::Caption,
        Role::Figure => Kind::Figure,
        Role::List => Kind::L,
        Role::ListItem => Kind::LI,
        Role::Label => Kind::Lbl,
        Role::ListBody => Kind::LBody,
        // Headings are told apart by the caller; an artifact is not planned.
        _ => Kind::P,
    }
}
