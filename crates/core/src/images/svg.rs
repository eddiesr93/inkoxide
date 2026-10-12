//! SVG parsing through `usvg`, with limits for untrusted input.
//!
//! The XML is parsed once with `roxmltree` (re-exported by `usvg`), checked,
//! then converted. The checks run before `usvg` builds its tree because
//! `usvg` expands `use` references and clones paint servers, clip paths
//! and masks per element, so a small file can describe a huge tree.
//!
//! Nothing is read from disk: `href`s to files are ignored and only
//! `data:` URLs are resolved. `usvg` resolves an image's URL again for
//! every copy, so a nested SVG is parsed once per copy; each parse goes
//! through the same checks and charges one [`Budget`] shared with the
//! SVG that holds it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use usvg::roxmltree::{self, Document, Node, NodeId};

/// Most elements the converted tree may contain, counting every copy made
/// by `use` and by `url(#…)` references, nested SVG images included.
const MAX_EXPANDED: usize = 250_000;
/// Most `data:` URL bytes decoded, counting every copy: `usvg` decodes the
/// URL again for each copy, and the PDF writer embeds each copy.
const MAX_DATA_BYTES: usize = 128 << 20;
/// Deepest element nesting, counting followed references and the elements
/// around nested SVG images (they are converted on the same stack).
const MAX_DEPTH: usize = 100;
/// Deepest SVG-in-SVG nesting. Real files embed an icon or a chart one
/// level deep; each level adds a parse and a conversion to the stack on
/// top of what [`MAX_DEPTH`] allows.
const MAX_NESTING: usize = 4;
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

/// What one SVG, its nested SVG images and all their copies may expand
/// to. Atomics because `usvg` wants a `Sync` resolver; one thread uses it.
#[derive(Default)]
struct Budget {
    elements: AtomicUsize,
    data_bytes: AtomicUsize,
    /// The first limit exceeded. A nested SVG that fails is left out, so
    /// this carries the error out to the outermost SVG.
    error: OnceLock<String>,
}

impl Budget {
    fn fail(&self, e: String) -> String {
        let _ = self.error.set(e.clone());
        e
    }

    fn charge_element(&self) -> Result<(), String> {
        if self.elements.fetch_add(1, Ordering::Relaxed) >= MAX_EXPANDED {
            return Err(self.fail(format!(
                "SVG is too complex: more than {MAX_EXPANDED} elements once references and \
                 nested SVG images are expanded"
            )));
        }
        Ok(())
    }

    fn charge_data(&self, bytes: usize) -> Result<(), String> {
        let total = self.data_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        if total > MAX_DATA_BYTES {
            return Err(self.fail(format!(
                "SVG is too complex: more than {} MiB of embedded images once references are \
                 expanded",
                MAX_DATA_BYTES >> 20
            )));
        }
        Ok(())
    }
}

/// Parses an SVG document into a `usvg` tree.
pub(crate) fn parse(bytes: &[u8]) -> Result<usvg::Tree, String> {
    let budget = Budget::default();
    let at = Nesting {
        budget: &budget,
        level: 0,
        depth: 0,
    };
    let tree = parse_nested(bytes, at);
    match budget.error.get() {
        Some(e) => Err(e.clone()),
        None => tree,
    }
}

/// Where an SVG sits inside the outermost one.
#[derive(Clone, Copy)]
struct Nesting<'b> {
    budget: &'b Budget,
    /// SVG images around it.
    level: usize,
    /// Element depth of its root.
    depth: usize,
}

fn parse_nested(bytes: &[u8], at: Nesting<'_>) -> Result<usvg::Tree, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG is not valid UTF-8".to_string())?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_XML_NODES,
        entity_resolver: None,
    };
    let doc = Document::parse_with_options(text, opts).map_err(|e| format!("SVG: {e}"))?;
    let images = check(&doc, at)?;
    // Nested SVGs start below the deepest image, wherever they are.
    let inner = Nesting {
        budget: at.budget,
        level: at.level + 1,
        depth: images + 1,
    };
    let options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(move |mime, data, _| resolve_data(mime, data, inner)),
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

/// `data:` URLs inside an SVG: JPEG and PNG are kept for the PDF writer
/// (which decodes them under the pixel limit), nested SVGs are parsed at
/// `at` with the same limits and budget, other formats are dropped.
fn resolve_data(mime: &str, data: Arc<Vec<u8>>, at: Nesting<'_>) -> Option<usvg::ImageKind> {
    // Over budget: the SVG fails anyway, so skip the remaining copies.
    if at.budget.error.get().is_some() {
        return None;
    }
    let nested = |data: &[u8]| {
        if at.level > MAX_NESTING {
            at.budget.fail(format!(
                "SVG is too complex: SVG images nest more than {MAX_NESTING} levels deep"
            ));
            return None;
        }
        let tree = parse_nested(data, at).ok()?;
        Some(usvg::ImageKind::SVG(tree))
    };
    match mime {
        "image/jpg" | "image/jpeg" => Some(usvg::ImageKind::JPEG(data)),
        "image/png" => Some(usvg::ImageKind::PNG(data)),
        "image/svg+xml" => nested(&data),
        "text/plain" | "" => {
            if data.starts_with(b"\x89PNG") {
                Some(usvg::ImageKind::PNG(data))
            } else if data.starts_with(&[0xFF, 0xD8]) {
                Some(usvg::ImageKind::JPEG(data))
            } else if is_svg(&data) {
                nested(&data)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Rejects input `usvg` would expand beyond the limits, and text, which
/// needs fonts this conversion does not have. Returns the depth of the
/// deepest element with a `data:` URL.
fn check(doc: &Document<'_>, at: Nesting<'_>) -> Result<usize, String> {
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
        budget: at.budget,
        images: 0,
        active: Vec::new(),
    };
    walk.visit(root, &[], at.depth)?;
    Ok(walk.images)
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

struct Walk<'d, 'i, 'b> {
    doc: &'d Document<'i>,
    ids: HashMap<&'d str, NodeId>,
    /// `url(#…)` targets in style sheets; charged to every shape since
    /// any selector may apply them.
    sheet_refs: Vec<&'d str>,
    budget: &'b Budget,
    /// Depth of the deepest element with a `data:` URL.
    images: usize,
    /// Elements being expanded (reference cycles are skipped, as `usvg`
    /// does).
    active: Vec<NodeId>,
}

impl<'d> Walk<'d, '_, '_> {
    /// Counts `n` and everything it pulls in. `inherited` are the
    /// `fill`/`stroke` references of its ancestors.
    fn visit(
        &mut self,
        n: Node<'d, '_>,
        inherited: &[&'d str],
        depth: usize,
    ) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(self
                .budget
                .fail(format!("SVG nests deeper than {MAX_DEPTH} levels")));
        }
        if self.active.contains(&n.id()) {
            return Ok(());
        }
        self.budget.charge_element()?;
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
                } else {
                    // Maybe a `data:` URL, decoded again for this copy.
                    self.budget.charge_data(v.len())?;
                    self.images = self.images.max(depth);
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

    /// `doc` as a percent-encoded `data:` URL, which nests without escaping.
    fn data_url(doc: &str) -> String {
        let mut url = String::from("data:image/svg+xml,");
        for b in doc.bytes() {
            if b.is_ascii_alphanumeric() {
                url.push(char::from(b));
            } else {
                let _ = write!(url, "%{b:02X}");
            }
        }
        url
    }

    /// An SVG drawing `copies` copies of `inner` as an `<image>`, inside
    /// `groups` nested groups.
    fn image_of(inner: &str, copies: usize, groups: usize) -> String {
        let mut body = format!(
            r#"<defs><image id="i" width="4" height="4" href="{}"/></defs>"#,
            data_url(inner)
        );
        body.push_str(&"<g>".repeat(groups));
        body.push_str(&r##"<use href="#i"/>"##.repeat(copies));
        body.push_str(&"</g>".repeat(groups));
        svg(&body)
    }

    /// `levels` of `<use>` bombs: about `10^levels` elements once expanded.
    fn bomb(levels: usize) -> String {
        let mut body = String::from(r#"<defs><rect id="l0" width="1" height="1"/>"#);
        for i in 1..=levels {
            let _ = write!(body, r#"<g id="l{i}">"#);
            body.push_str(&format!(r##"<use href="#l{}"/>"##, i - 1).repeat(10));
            body.push_str("</g>");
        }
        let _ = write!(body, r##"</defs><use href="#l{levels}"/>"##);
        svg(&body)
    }

    /// Nested SVG images: (copies at the top level, deepest level).
    fn nested_images(g: &usvg::Group) -> (usize, usize) {
        let (mut copies, mut deepest) = (0, 0);
        for n in g.children() {
            match n {
                usvg::Node::Group(g) => {
                    let (c, d) = nested_images(g);
                    copies += c;
                    deepest = deepest.max(d);
                }
                usvg::Node::Image(img) => {
                    if let usvg::ImageKind::SVG(t) = img.kind() {
                        copies += 1;
                        deepest = deepest.max(1 + nested_images(t.root()).1);
                    }
                }
                _ => {}
            }
        }
        (copies, deepest)
    }

    #[test]
    fn nested_svgs_share_the_budget() {
        // Each copy of a nested SVG is parsed again: a thousand copies of
        // 47k elements must not get a fresh budget each.
        let inner = bomb(4);
        let t = std::time::Instant::now();
        let e = parse(image_of(&inner, 1000, 0).as_bytes()).unwrap_err();
        assert!(e.contains("too complex"), "{e}");
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());

        // Nor each level: 50 copies of 50 copies of 4.7k elements.
        let mid = image_of(&bomb(3), 50, 0);
        let e = parse(image_of(&mid, 50, 0).as_bytes()).unwrap_err();
        assert!(e.contains("too complex"), "{e}");

        // A few copies fit.
        let t = parse(image_of(&inner, 3, 0).as_bytes()).unwrap();
        assert_eq!(nested_images(t.root()), (3, 1));
    }

    #[test]
    fn caps_svg_in_svg_nesting() {
        let chain = |levels: usize, groups: usize| {
            let mut doc = svg(r#"<rect width="5" height="5"/>"#);
            for _ in 0..levels {
                doc = image_of(&doc, 1, groups);
            }
            doc
        };
        let e = parse(chain(MAX_NESTING + 1, 0).as_bytes()).unwrap_err();
        assert!(e.contains("too complex") && e.contains("nest"), "{e}");

        // Element depth adds up across levels: each level is its groups,
        // root, `use` and image deep, and the innermost has a rect.
        let groups = (MAX_DEPTH - 1) / MAX_NESTING - 3;
        let e = parse(chain(MAX_NESTING, groups + 1).as_bytes()).unwrap_err();
        assert!(e.contains("deeper"), "{e}");

        // The deepest accepted chain converts on a small stack.
        let doc = chain(MAX_NESTING, groups);
        let t = std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || parse(doc.as_bytes()).unwrap())
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(nested_images(t.root()).1, MAX_NESTING);
    }

    #[test]
    fn limits_embedded_image_bytes() {
        // `usvg` decodes the URL again for every copy.
        let png = format!("data:image/png;base64,{}", "A".repeat(1 << 20));
        let body = format!(r#"<defs><image id="i" width="4" height="4" href="{png}"/></defs>"#);
        let e = parse(svg(&(body.clone() + &r##"<use href="#i"/>"##.repeat(200))).as_bytes())
            .unwrap_err();
        assert!(e.contains("MiB"), "{e}");
        parse(svg(&(body + &r##"<use href="#i"/>"##.repeat(2))).as_bytes()).unwrap();
    }
}
