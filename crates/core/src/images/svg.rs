//! SVG parsing through `usvg`, with limits for untrusted input.
//!
//! The XML is parsed once with `roxmltree` (re-exported by `usvg`), checked,
//! then converted. The checks run before `usvg` builds its tree because
//! `usvg` expands `use` references and clones paint servers, clip paths
//! and masks per element, so a small file can describe a huge tree.
//!
//! Nothing is read from disk: `href`s to files are ignored and only
//! `data:` URLs are resolved (nested SVGs go through the same checks).

use std::collections::HashMap;
use std::sync::Arc;

use usvg::roxmltree::{self, Document, Node, NodeId};

/// Most elements the converted tree may contain, counting every copy made
/// by `use` and by `url(#…)` references.
const MAX_EXPANDED: usize = 250_000;
/// Deepest element nesting, counting followed references.
const MAX_DEPTH: usize = 100;
/// XML nodes accepted by the parser.
const MAX_XML_NODES: u32 = 1_000_000;
/// Largest intrinsic size, in CSS pixels per side.
const MAX_SIDE: f32 = 100_000.0;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Looks like SVG: optional BOM and whitespace, then `<`.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = b.iter().position(|c| !c.is_ascii_whitespace());
    start.is_some_and(|i| b[i] == b'<')
}

/// Parses an SVG document into a `usvg` tree.
pub(crate) fn parse(bytes: &[u8]) -> Result<usvg::Tree, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG is not valid UTF-8".to_string())?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_XML_NODES,
        entity_resolver: None,
    };
    let doc = Document::parse_with_options(text, opts).map_err(|e| format!("SVG: {e}"))?;
    check(&doc)?;
    let options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(resolve_data),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_xmltree(&doc, &options).map_err(|e| format!("SVG: {e}"))?;
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err("SVG has no usable size".into());
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(format!("SVG is {w}×{h} px; at most {MAX_SIDE} px per side"));
    }
    Ok(tree)
}

/// `data:` URLs inside the SVG: JPEG and PNG are kept for the PDF writer,
/// nested SVGs are parsed with the same limits, other formats are dropped.
fn resolve_data(mime: &str, data: Arc<Vec<u8>>, _: &usvg::Options) -> Option<usvg::ImageKind> {
    match mime {
        "image/jpg" | "image/jpeg" => Some(usvg::ImageKind::JPEG(data)),
        "image/png" => Some(usvg::ImageKind::PNG(data)),
        "image/svg+xml" => parse(&data).ok().map(usvg::ImageKind::SVG),
        "text/plain" | "" => {
            if data.starts_with(b"\x89PNG") {
                Some(usvg::ImageKind::PNG(data))
            } else if data.starts_with(&[0xFF, 0xD8]) {
                Some(usvg::ImageKind::JPEG(data))
            } else if is_svg(&data) {
                parse(&data).ok().map(usvg::ImageKind::SVG)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Rejects input `usvg` would expand beyond the limits, and text, which
/// needs fonts this conversion does not have.
fn check(doc: &Document<'_>) -> Result<(), String> {
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err("SVG: the root element is not <svg>".into());
    }
    let mut ids: HashMap<&str, NodeId> = HashMap::new();
    let mut sheet_refs: Vec<&str> = Vec::new();
    for n in root.descendants().filter(Node::is_element) {
        if let Some(id) = n.attribute("id") {
            ids.entry(id).or_insert(n.id());
        }
        let name = n.tag_name().name();
        if is_svg_ns(n) && name == "text" && n.descendants().any(|d| has_text(&d)) {
            return Err("SVG <text> is not supported; convert the text to outlines (paths)".into());
        }
        if name == "style" {
            for t in n.descendants().filter_map(|d| d.text()) {
                sheet_refs.extend(url_refs(t));
            }
        }
    }
    let mut walk = Walk {
        doc,
        ids,
        sheet_refs,
        count: 0,
        active: Vec::new(),
    };
    walk.visit(root, &[], 0)
}

fn is_svg_ns(n: Node<'_, '_>) -> bool {
    matches!(n.tag_name().namespace(), Some(SVG_NS) | None)
}

fn has_text(n: &Node<'_, '_>) -> bool {
    n.is_text() && n.text().is_some_and(|t| !t.trim().is_empty())
}

/// Ids named by `url(#id)` in a value.
fn url_refs(v: &str) -> impl Iterator<Item = &str> {
    v.split("url(").skip(1).filter_map(|rest| {
        let inner = rest.split(')').next()?;
        let inner = inner.trim().trim_matches(|c| c == '"' || c == '\'');
        inner.strip_prefix('#')
    })
}

const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "image",
];

struct Walk<'d, 'i> {
    doc: &'d Document<'i>,
    ids: HashMap<&'d str, NodeId>,
    /// `url(#…)` targets in style sheets; charged to every shape since
    /// any selector may apply them.
    sheet_refs: Vec<&'d str>,
    count: usize,
    /// Elements being expanded (reference cycles are skipped, as `usvg`
    /// does).
    active: Vec<NodeId>,
}

impl<'d> Walk<'d, '_> {
    fn bump(&mut self) -> Result<(), String> {
        self.count += 1;
        if self.count > MAX_EXPANDED {
            return Err(format!(
                "SVG is too complex: more than {MAX_EXPANDED} elements once references are expanded"
            ));
        }
        Ok(())
    }

    /// Counts `n` and everything it pulls in. `inherited` are the
    /// `fill`/`stroke` references of its ancestors.
    fn visit(
        &mut self,
        n: Node<'d, '_>,
        inherited: &[&'d str],
        depth: usize,
    ) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(format!("SVG nests deeper than {MAX_DEPTH} levels"));
        }
        if self.active.contains(&n.id()) {
            return Ok(());
        }
        self.bump()?;
        self.active.push(n.id());
        let mut refs: Vec<&'d str> = Vec::new();
        let mut paint: Vec<&'d str> = inherited.to_vec();
        for a in n.attributes() {
            let v: &'d str = a.value();
            let name = a.name();
            for r in url_refs(v) {
                if matches!(name, "fill" | "stroke" | "style") {
                    paint.push(r);
                } else {
                    refs.push(r);
                }
            }
            let is_href = name == "href" && matches!(a.namespace(), None | Some(XLINK_NS));
            if is_href {
                if let Some(r) = v.strip_prefix('#') {
                    refs.push(r);
                }
            }
        }
        if SHAPES.contains(&n.tag_name().name()) {
            refs.extend(paint.iter().copied());
            refs.extend(self.sheet_refs.clone());
        }
        for r in refs {
            if let Some(&id) = self.ids.get(r) {
                if let Some(target) = self.doc.get_node(id) {
                    self.visit(target, &[], depth + 1)?;
                }
            }
        }
        for c in n.children().filter(Node::is_element) {
            self.visit(c, &paint, depth + 1)?;
        }
        self.active.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn svg(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">{body}</svg>"#)
    }

    #[test]
    fn parses_and_sizes() {
        let t = parse(svg(r#"<rect width="5" height="5" fill="red"/>"#).as_bytes()).unwrap();
        assert_eq!((t.size().width(), t.size().height()), (20.0, 10.0));
        assert!(is_svg(b"\xEF\xBB\xBF  <?xml version=\"1.0\"?><svg/>"));
        assert!(!is_svg(b"\x89PNG"));
    }

    #[test]
    fn rejects_text_and_garbage() {
        let e = parse(svg("<text x='1' y='5'>Hi</text>").as_bytes()).unwrap_err();
        assert!(e.contains("<text>"), "{e}");
        // Empty text elements are harmless.
        parse(svg("<text x='1' y='5'> </text>").as_bytes()).unwrap();
        assert!(parse(b"<svg").is_err());
        assert!(parse(b"<html/>").is_err());
        assert!(parse(&[0xFF, 0xFE, 0x3C]).is_err());
    }

    #[test]
    fn rejects_reference_bombs() {
        // Ten levels of ten `use`s: 10^10 elements once expanded.
        let mut body = String::from(r#"<defs><rect id="l0" width="1" height="1"/>"#);
        for i in 1..=10 {
            let _ = write!(body, r#"<g id="l{i}">"#);
            for _ in 0..10 {
                let _ = write!(body, r##"<use href="#l{}"/>"##, i - 1);
            }
            body.push_str("</g>");
        }
        body.push_str(r##"</defs><use href="#l10"/>"##);
        let e = parse(svg(&body).as_bytes()).unwrap_err();
        assert!(e.contains("too complex"), "{e}");

        // Self-reference is not a bomb.
        parse(svg(r##"<g id="a"><use href="#a"/></g>"##).as_bytes()).unwrap();

        let mut deep = String::new();
        for _ in 0..(MAX_DEPTH + 5) {
            deep.push_str("<g>");
        }
        for _ in 0..(MAX_DEPTH + 5) {
            deep.push_str("</g>");
        }
        let e = parse(svg(&deep).as_bytes()).unwrap_err();
        assert!(e.contains("deeper"), "{e}");
    }

    #[test]
    fn ignores_files() {
        let t =
            parse(svg(r#"<image href="/etc/passwd" width="5" height="5"/>"#).as_bytes()).unwrap();
        assert!(t.root().children().is_empty());
    }
}
