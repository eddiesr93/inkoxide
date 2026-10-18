//! The checks of tagged output: what roles nodes take, and what PDF/UA-1
//! asks of the nodes it reads.

use super::Checker;
use crate::error::{ValidationCode as Code, ValidationError};
use crate::schema::{Node, NodeKind, Role};
use crate::tag::Kind;

/// Where a node is in the structure tagged output makes of the document.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Structure {
    /// The role of the nearest node around it with one: its element's
    /// parent (`None`: the document, or a table cell).
    parent: Option<Role>,
    /// Inside an element that holds content (a paragraph, a figure): the
    /// roles inside make no elements.
    merged: bool,
}

impl Structure {
    /// Where the children of `node` are.
    pub(super) fn enter(self, node: &Node) -> Self {
        if self.merged {
            return self;
        }
        match (&node.kind, node.role) {
            // Its cells are elements of their own.
            (NodeKind::Table(_), _) => Self::default(),
            (_, Some(role)) => Self {
                parent: Some(role),
                merged: Kind::of(role).is_some_and(Kind::holds_content),
            },
            _ => self,
        }
    }
}

impl Checker<'_> {
    /// A node's `role`: one its type can have. A text is a heading, a
    /// paragraph, a caption, a label, a quote or decoration; an image a
    /// figure or decoration; a box anything.
    pub(super) fn role(&mut self, role: Role, kind: &NodeKind) {
        let ok = match kind {
            NodeKind::Box { .. } => true,
            NodeKind::Text { .. } => {
                role.heading().is_some()
                    || matches!(
                        role,
                        Role::P | Role::Caption | Role::Label | Role::BlockQuote | Role::Artifact
                    )
            }
            NodeKind::Image { .. } => matches!(role, Role::Figure | Role::Artifact),
            _ => false,
        };
        if !ok {
            self.err(
                Code::InvalidValue,
                Some("role"),
                "a text takes h1 to h6, p, caption, label, blockQuote or artifact; an image figure or artifact",
            );
        }
    }

    /// The first thing PDF/UA-1 asks of the document that it lacks: the
    /// render's conflict, at `field` of the node checked.
    pub(super) fn lacks(&mut self, field: &str, message: String) {
        if self.conflict.is_none() {
            self.conflict = Some(ValidationError {
                code: Code::UaConflict,
                path: format!("{}.{field}", self.path),
                message,
            });
        }
    }

    /// What PDF/UA-1 asks of a node: a label on a field, wherever it is
    /// (its widget is tagged even in decoration); and when tagged output
    /// reads it (`read`), alternative text on images (in text too) and
    /// figures, headings that go down one level at a time from `h1`, and
    /// lists made of items, each with a body.
    pub(super) fn ua_node(&mut self, node: &Node, read: bool) {
        const NO_ALT: &str =
            "PDF/UA requires alternative text on an image: `alt`, or \"\" for decoration";
        if let NodeKind::Field(f) = &node.kind {
            if f.label.is_none() {
                self.lacks(
                    "field.label",
                    "PDF/UA requires a label on a form field: what it is for".into(),
                );
            }
        }
        if !read {
            return;
        }
        match &node.kind {
            NodeKind::Image { alt: None, .. } => self.lacks("alt", NO_ALT.into()),
            NodeKind::Text { runs } => {
                if let Some(i) = runs
                    .iter()
                    .position(|r| r.image.as_ref().is_some_and(|im| im.alt.is_none()))
                {
                    self.lacks(&format!("runs[{i}].image.alt"), NO_ALT.into());
                }
            }
            NodeKind::Box { alt: None, .. } if node.role == Some(Role::Figure) => self.lacks(
                "alt",
                "PDF/UA requires alternative text on a figure: `alt`".into(),
            ),
            _ => {}
        }
        if let Some(level) = node.role.and_then(Role::heading) {
            let ok = match self.last_heading {
                None => level == 1,
                Some(prev) => level <= prev + 1,
            };
            if !ok {
                let after = self
                    .last_heading
                    .map_or("the start".to_string(), |p| format!("h{p}"));
                self.lacks(
                    "role",
                    format!(
                        "PDF/UA headings go down one level at a time from h1: h{level} after {after}"
                    ),
                );
            }
            self.last_heading = Some(level);
        }
        // ISO 14289-1, 7.6: a list holds items, an item a label and a body.
        let parent = (!self.structure.merged).then_some(self.structure.parent);
        let inside = match node.role {
            Some(Role::ListItem) => Some((Role::List, "a `listItem` is inside a `list`")),
            Some(Role::ListBody) => Some((Role::ListItem, "a `listBody` is inside a `listItem`")),
            _ => None,
        };
        if let (Some(parent), Some((want, message))) = (parent, inside) {
            if parent != Some(want) {
                self.lacks("role", format!("PDF/UA lists are made of items: {message}"));
            }
        }
    }
}
