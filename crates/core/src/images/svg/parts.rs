//! The parts of an SVG that tagged output tags on their own: an element
//! with a name of its own (`aria-label`, or else a `title` child) is a
//! figure inside the image's, and an element with `aria-hidden="true"` is
//! decoration. `usvg` keeps neither the names nor the attributes, but it
//! keeps the `id` of groups, shapes, images and text, so a part is known
//! by its `id`: one that has none is given one before conversion.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use usvg::roxmltree::{Document, Node};

use super::SVG_NS;

/// What tagged output makes of a part of an SVG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    /// A figure of its own, with this alternative text.
    Named(String),
    /// Decoration: an artifact.
    Hidden,
}

/// The parts of an SVG, by the `id` of their element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SvgParts {
    pub by_id: HashMap<String, Part>,
}

impl SvgParts {
    /// Without the parts whose `id`s are those of `ids`, given by
    /// [`find`].
    pub(super) fn without(mut self, ids: &[(usize, String)]) -> Self {
        for (_, id) in ids {
            self.by_id.remove(id);
        }
        self
    }
}

/// The elements `usvg` keeps with their `id`: groups, `use`, shapes,
/// images and text.
const KEPT: &[&str] = &[
    "g", "use", "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "image",
    "text", "a",
];

/// What is never drawn where it is, so has no part of its own.
const UNDRAWN: &[&str] = &[
    "defs",
    "symbol",
    "clipPath",
    "mask",
    "marker",
    "pattern",
    "linearGradient",
    "radialGradient",
    "filter",
];

/// The parts of `doc`, and the `id`s to give the elements of the parts that
/// have none, each with where it goes in the source (after the element's
/// name); `None` if it has no part.
pub(super) fn find(doc: &Document<'_>) -> Option<(SvgParts, Vec<(usize, String)>)> {
    let mut parts = SvgParts::default();
    let mut missing = Vec::new();
    let root = doc.root_element();
    for n in root.descendants().skip(1) {
        if !n.is_element() || n.tag_name().namespace() != Some(SVG_NS) {
            continue;
        }
        if !KEPT.contains(&n.tag_name().name()) {
            continue;
        }
        let Some(part) = part(n) else { continue };
        if n.ancestors().any(|a| {
            a.tag_name().namespace() == Some(SVG_NS) && UNDRAWN.contains(&a.tag_name().name())
        }) {
            continue;
        }
        match n.attribute("id") {
            // An empty `id` cannot take another: not a part.
            Some("") => {}
            Some(id) => {
                parts.by_id.insert(id.to_string(), part);
            }
            // An element of an entity's value has its text in the DTD,
            // where no `id` can be written for it alone.
            None if n.range().start < root.range().start => {}
            None => missing.push((n, part)),
        }
    }
    if parts.by_id.is_empty() && missing.is_empty() {
        return None;
    }
    let taken: HashSet<&str> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    let mut next = 0usize;
    let mut ids = Vec::with_capacity(missing.len());
    for (n, part) in missing {
        let id = loop {
            let id = format!("inkoxide-part-{next}");
            next += 1;
            if !taken.contains(id.as_str()) {
                break id;
            }
        };
        ids.push((after_name(doc.input_text(), n.range().start), id.clone()));
        parts.by_id.insert(id, part);
    }
    Some((parts, ids))
}

/// `text` with each `id` of `ids` written at its place.
pub(super) fn named(text: &str, ids: &[(usize, String)]) -> String {
    let mut out = String::with_capacity(text.len() + ids.len() * 32);
    let mut at = 0;
    let mut sorted: Vec<&(usize, String)> = ids.iter().collect();
    sorted.sort_by_key(|(i, _)| *i);
    for (i, id) in sorted {
        out.push_str(&text[at..*i]);
        let _ = write!(out, " id=\"{id}\"");
        at = *i;
    }
    out.push_str(&text[at..]);
    out
}

/// Where the name of the element whose start tag is at `start` ends.
fn after_name(text: &str, start: usize) -> usize {
    let tag = &text[start + 1..];
    start
        + 1
        + tag
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(tag.len())
}

/// What element `n` is as a part, if one.
fn part(n: Node<'_, '_>) -> Option<Part> {
    if n.attribute("aria-hidden") == Some("true") {
        return Some(Part::Hidden);
    }
    let label = n
        .attribute("aria-label")
        .map(collapse)
        .filter(|l| !l.is_empty());
    let title = || {
        n.children()
            .find(|c| c.has_tag_name((SVG_NS, "title")))
            .map(|t| {
                collapse(
                    &t.descendants()
                        .filter(Node::is_text)
                        .filter_map(|d| d.text())
                        .collect::<String>(),
                )
            })
            .filter(|t| !t.is_empty())
    };
    label.or_else(title).map(Part::Named)
}

/// `s` with its runs of white space made one space, and trimmed.
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(svg: &str) -> Document<'_> {
        let options = usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        Document::parse_with_options(svg, options).unwrap()
    }

    fn parts(svg: &str) -> Option<(SvgParts, String)> {
        let doc = parse(svg);
        let (parts, ids) = find(&doc)?;
        Some((parts, named(svg, &ids)))
    }

    #[test]
    fn named_and_hidden_elements_are_parts() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><title>All</title><g id="bars" aria-label=" Sales
            by month "><rect width="1" height="1"/></g><circle r="1"><title>A dot</title></circle><path d="M0 0" aria-hidden="true"/><defs><rect id="r" aria-label="unused"/></defs><rect aria-label=""/></svg>"#;
        let (parts, named) = parts(svg).unwrap();
        assert_eq!(parts.by_id.len(), 3);
        assert_eq!(parts.by_id["bars"], Part::Named("Sales by month".into()));
        assert_eq!(parts.by_id["inkoxide-part-0"], Part::Named("A dot".into()));
        assert_eq!(parts.by_id["inkoxide-part-1"], Part::Hidden);
        assert!(named.contains(r#"<circle id="inkoxide-part-0" r="1">"#));
        assert!(named.contains(r#"<path id="inkoxide-part-1" d="M0 0""#));
    }

    #[test]
    fn given_ids_do_not_collide() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g id="inkoxide-part-0"/><rect aria-label="R"/></svg>"#;
        let (parts, named) = parts(svg).unwrap();
        assert_eq!(parts.by_id["inkoxide-part-1"], Part::Named("R".into()));
        assert!(named.contains(r#"<rect id="inkoxide-part-1" aria-label="R"/>"#));
    }

    #[test]
    fn elements_that_cannot_take_an_id_are_no_parts() {
        let svg = r#"<!DOCTYPE svg [<!ENTITY e "<rect aria-label='E'/>">]><svg xmlns="http://www.w3.org/2000/svg">&e;&e;<rect id="" aria-label="Empty"/><g aria-label="G"/></svg>"#;
        let (parts, named) = parts(svg).unwrap();
        assert_eq!(parts.by_id.len(), 1);
        assert_eq!(parts.by_id["inkoxide-part-0"], Part::Named("G".into()));
        parse(&named);
    }

    #[test]
    fn an_svg_without_parts_has_none() {
        assert!(
            parts(r#"<svg xmlns="http://www.w3.org/2000/svg"><title>T</title><rect/></svg>"#)
                .is_none()
        );
    }
}
