//! SVG parsing through `usvg`, with limits for untrusted input.
//!
//! The XML is parsed once with `roxmltree` (re-exported by `usvg`), checked,
//! then converted. The checks run before `usvg` builds its tree because
//! `usvg` expands `use` references, draws markers once per vertex and
//! clones paint servers, clip paths and masks per element, so a small file
//! can describe a huge tree.
//!
//! Nothing is read from disk: `href`s to files and URLs are ignored and
//! only `data:` URLs are resolved. The checks count a nested SVG image once per
//! copy, its own nested images included. `usvg` resolves the URL again
//! for most copies, so the resolver also counts what it actually decodes,
//! in one [`Meter`] for the whole SVG.
//!
//! Once converted, the path segments the tree draws are counted as the PDF
//! writer draws them (`limits::drawn_segments`), so that an SVG the
//! writer would refuse fails here, where validation sees it.
//!
//! Where mirroring `usvg` would be fragile, the input is refused instead:
//! entities that expand to more than a little text, `href` chains that
//! loop, and radii so large that an arc becomes millions of curves.

use std::collections::HashMap;
use std::sync::Arc;

use usvg::roxmltree::{self, Document};

use super::probe::{sniff, ImageFormat};
use super::ContentKey;

mod check;
mod limits;
mod parts;
mod text;
mod view_box;

use check::check;
pub(crate) use check::iri;
pub(crate) use limits::MAX_SEGMENTS;
use limits::{
    too_complex, Meter, Reject, MAX_ENTITY_TEXT, MAX_NESTING, MAX_SIDE, MAX_XML_DEPTH,
    MAX_XML_NODES, MIN_SIDE,
};
pub(crate) use parts::{Part, SvgParts};
pub(crate) use text::*;
pub(crate) use view_box::*;

pub(crate) const SVG_NS: &str = "http://www.w3.org/2000/svg";

pub(crate) const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Looks like SVG: optional BOM and whitespace, then `<`.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = b.iter().position(|c| !c.is_ascii_whitespace());
    start.is_some_and(|i| b[i] == b'<')
}

/// Parses an SVG document into a `usvg` tree without its text, with its
/// root `viewBox`.
pub(crate) fn parse(bytes: &[u8]) -> Result<(usvg::Tree, Option<ViewBox>), String> {
    let parsed = parse_text(bytes)?;
    Ok((parsed.tree, parsed.view_box))
}

/// Parses an SVG document into a `usvg` tree, with its root `viewBox` and
/// what converting its text needs, when it has text: the tree leaves the
/// text out.
pub(crate) fn parse_text(bytes: &[u8]) -> Result<Parsed, String> {
    let meter = Meter::default();
    let at = Nesting {
        meter: &meter,
        level: 0,
        depth: 0,
        rewrites: None,
    };
    let parsed = parse_nested(bytes, at).map_err(Reject::message);
    match meter.error.get() {
        Some(e) => Err(e.clone()),
        None => parsed,
    }
}

/// Where an SVG sits inside the outermost one.
#[derive(Clone, Copy)]
struct Nesting<'m> {
    meter: &'m Meter,
    /// SVG images around it.
    level: usize,
    /// Element depth of its root.
    depth: usize,
    /// Nested SVGs to convert in place of others: their text replaced.
    rewrites: Option<&'m Rewrites>,
}

/// Parses `bytes` as XML, with the limits on nodes and entities of an SVG
/// image; the message of what failed.
pub(crate) fn parse_document(bytes: &[u8]) -> Result<Document<'_>, String> {
    parse_xml(bytes).map_err(Reject::message)
}

fn parse_xml(bytes: &[u8]) -> Result<Document<'_>, Reject> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Reject::Invalid("SVG is not valid UTF-8".into()))?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    // The XML parser recurses per element, an entity's elements too: a
    // deep document would overflow the stack before `check` sees it.
    let depth = tag_depth(text).saturating_add(check_entities(text)?);
    if depth > MAX_XML_DEPTH {
        return Err(Reject::Limit(format!(
            "SVG nests deeper than {MAX_XML_DEPTH} levels"
        )));
    }
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_XML_NODES,
        entity_resolver: None,
    };
    Document::parse_with_options(text, opts).map_err(|e| Reject::Invalid(format!("SVG: {e}")))
}

/// The deepest tags of `text` nest, or more: a start tag that never ends
/// counts as open, and so do the tags inside a declaration's values.
fn tag_depth(text: &str) -> usize {
    let b = text.as_bytes();
    let skip = |from: usize, end: &[u8]| {
        b[from..]
            .windows(end.len())
            .position(|w| w == end)
            .map_or(b.len(), |p| from + p + end.len())
    };
    let (mut depth, mut peak, mut i) = (0usize, 0usize, 0usize);
    while let Some(p) = b[i..].iter().position(|&c| c == b'<') {
        i += p;
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            i = skip(i + 4, b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = skip(i + 9, b"]]>");
        } else if rest.starts_with(b"<?") {
            i = skip(i + 2, b"?>");
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            i += 2;
        } else if rest.starts_with(b"<!") {
            // A declaration: what its values hold is counted as it comes.
            i += 2;
        } else {
            // A start tag: its end, past quoted values; `/>` closes it.
            let mut quote = None;
            let mut end = rest.len();
            for (k, &c) in rest.iter().enumerate().skip(1) {
                match (quote, c) {
                    (Some(q), c) if c == q => quote = None,
                    (None, b'"' | b'\'') => quote = Some(c),
                    (None, b'>') => {
                        end = k;
                        break;
                    }
                    _ => {}
                }
            }
            if end == rest.len() || rest[end - 1] != b'/' {
                depth += 1;
                peak = peak.max(depth);
            }
            i += end.min(rest.len() - 1) + 1;
        }
    }
    peak
}

/// Refuses entities that expand to more than [`MAX_ENTITY_TEXT`] in all,
/// before the XML parser expands them. Entities may not refer to other
/// entities, so each reference expands to its own value; the short
/// namespace entities of drawing programs pass. The deepest the tags of
/// a value nest: a reference puts them that much deeper.
fn check_entities(text: &str) -> Result<usize, Reject> {
    if !text.contains("<!ENTITY") {
        return Ok(0);
    }
    let mut deepest = 0;
    let mut values: HashMap<&str, usize> = HashMap::new();
    for decl in text.split("<!ENTITY").skip(1) {
        let decl = decl.trim_start();
        if decl.starts_with('%') {
            return Err(Reject::Invalid(
                "SVG: parameter entities are not supported".into(),
            ));
        }
        let name_end = decl
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(decl.len());
        let (name, rest) = decl.split_at(name_end);
        let rest = rest.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            // External entities are not resolved.
            continue;
        };
        let value = rest[1..].split(quote).next().unwrap_or("");
        if value.contains(['&', '%']) {
            return Err(too_complex("an entity refers to other entities"));
        }
        values.insert(name, value.len());
        deepest = deepest.max(tag_depth(value));
    }
    let mut total = 0usize;
    for r in text.split('&').skip(1) {
        let Some((name, _)) = r.split_once(';') else {
            continue;
        };
        if let Some(&len) = values.get(name) {
            total = total.saturating_add(len);
            if total > MAX_ENTITY_TEXT {
                return Err(too_complex(&format!(
                    "entities expand to more than {} MiB",
                    MAX_ENTITY_TEXT >> 20
                )));
            }
        }
    }
    Ok(deepest)
}

/// A parsed SVG.
#[derive(Debug)]
pub(crate) struct Parsed {
    pub tree: usvg::Tree,
    /// Its root `viewBox`.
    pub view_box: Option<ViewBox>,
    /// What converting its text needs, when it or a nested SVG has text.
    pub text: Option<TextSource>,
    /// Its parts, when it has any (the outermost SVG only).
    pub parts: Option<SvgParts>,
}

fn parse_nested(bytes: &[u8], at: Nesting<'_>) -> Result<Parsed, Reject> {
    parse_with_parts(bytes, at, None)
}

/// [`parse_nested`], with the parts of `bytes` when they are known: those
/// of the SVG it was made from, with `id`s given.
fn parse_with_parts(
    bytes: &[u8],
    at: Nesting<'_>,
    known: Option<SvgParts>,
) -> Result<Parsed, Reject> {
    let doc = parse_xml(bytes)?;
    // The parts of the SVG an image is: the elements of those without an
    // `id` get one, and the SVG is parsed again.
    let parts = match (known, at.level, at.rewrites) {
        (Some(known), ..) => Some(known),
        (None, 0, None) => match parts::find(&doc) {
            Some((found, ids)) if !ids.is_empty() => {
                let named = parts::named(doc.input_text(), &ids);
                match parse_with_parts(named.as_bytes(), at, Some(found.clone())) {
                    // Should the `id`s break it, without those parts.
                    Err(Reject::Invalid(_)) => {
                        Some(found.without(&ids)).filter(|p| !p.by_id.is_empty())
                    }
                    parsed => return parsed,
                }
            }
            found => found.map(|(p, _)| p),
        },
        _ => None,
    };
    let checked = check(&doc, at.level, at.depth)?;
    if !checked.text.texts.is_empty() {
        check_placeholders(bytes, &checked.text, at)?;
    }
    at.meter.charge(checked.own, 0)?;
    // Nested SVGs start below the deepest image, wherever they are.
    let inner = Nesting {
        meter: at.meter,
        level: at.level + 1,
        depth: checked.image_depth + 1,
        rewrites: at.rewrites,
    };
    let options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(move |mime, data, _| resolve_data(mime, data, inner)),
            resolve_string: Box::new(|_, _| None),
        },
        // As text is laid out (`svg_text`), for `em` lengths.
        font_size: super::svg_text::DEFAULT_FONT_SIZE,
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_xmltree(&doc, &options)
        .map_err(|e| Reject::Invalid(format!("SVG: {e}")))?;
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(Reject::Invalid("SVG has no usable size".into()));
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(Reject::Invalid(format!(
            "SVG is {w}×{h} px; at most {MAX_SIDE} px per side"
        )));
    }
    if w < MIN_SIDE || h < MIN_SIDE {
        return Err(Reject::Invalid(format!(
            "SVG is {w}×{h} px; at least {MIN_SIDE} px per side"
        )));
    }
    let root = doc.root_element();
    // One of zero or negative size too, which `usvg` ignores.
    if let Some([.., vw, vh]) = ViewBox::rect_of(root) {
        if vw < MIN_SIDE || vh < MIN_SIDE {
            return Err(Reject::Invalid(format!(
                "SVG viewBox is {vw}×{vh}; at least {MIN_SIDE} per side"
            )));
        }
    }
    let view_box = ViewBox::of(root);
    // As decoded: a nested SVG counts where it is drawn, and a copy with
    // its text converted for a render is held to it as it is written.
    if at.level == 0
        && at.rewrites.is_none()
        && limits::drawn_segments(tree.root(), false) > MAX_SEGMENTS
    {
        return Err(too_complex(&format!(
            "more than {MAX_SEGMENTS} path segments once references are expanded"
        )));
    }
    let text = TextSource::new(Arc::new(bytes.to_vec()), checked);
    Ok(Parsed {
        tree,
        view_box,
        text,
        parts,
    })
}

/// What a decoded `data:` URL holds, as the resolver reads it.
fn embedded(mime: &str, data: &[u8]) -> Option<ImageFormat> {
    match mime {
        "image/jpg" | "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/png" => Some(ImageFormat::Png),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::WebP),
        "image/svg+xml" => Some(ImageFormat::Svg),
        "text/plain" | "" => sniff(data),
        _ => None,
    }
}

fn too_deep() -> Reject {
    too_complex(&format!(
        "SVG images nest more than {MAX_NESTING} levels deep"
    ))
}

/// `data:` URLs inside an SVG: JPEG, PNG, GIF and WebP are kept for the PDF
/// writer, nested SVGs are parsed at `at` with the same limits, other
/// formats are dropped. Everything is charged to the meter, every time.
fn resolve_data(mime: &str, data: Arc<Vec<u8>>, at: Nesting<'_>) -> Option<usvg::ImageKind> {
    // Over a limit: the SVG fails anyway, so skip the remaining copies.
    if at.meter.error.get().is_some() {
        return None;
    }
    let kind = at
        .meter
        .charge(0, data.len())
        .and_then(|()| resolve(mime, data, at));
    match kind {
        Ok(kind) => kind,
        Err(Reject::Limit(e) | Reject::Unsupported(e)) => {
            let _ = at.meter.error.set(e);
            None
        }
        Err(Reject::Invalid(_)) => None,
    }
}

fn resolve(
    mime: &str,
    data: Arc<Vec<u8>>,
    at: Nesting<'_>,
) -> Result<Option<usvg::ImageKind>, Reject> {
    Ok(match embedded(mime, &data) {
        Some(ImageFormat::Jpeg) => raster(&data, at.meter)?.then_some(usvg::ImageKind::JPEG(data)),
        Some(ImageFormat::Png) => raster(&data, at.meter)?.then_some(usvg::ImageKind::PNG(data)),
        Some(ImageFormat::Gif) => raster(&data, at.meter)?.then_some(usvg::ImageKind::GIF(data)),
        Some(ImageFormat::WebP) => raster(&data, at.meter)?.then_some(usvg::ImageKind::WEBP(data)),
        Some(ImageFormat::Svg) => {
            if at.level > MAX_NESTING {
                return Err(too_deep());
            }
            let data = at
                .rewrites
                .and_then(|r| r.get(&ContentKey::new(data.clone())))
                .cloned()
                .unwrap_or(data);
            Some(usvg::ImageKind::SVG(parse_nested(&data, at)?.tree))
        }
        None => None,
    })
}

/// Reads a raster image's header and charges its pixels. `false` leaves
/// out what the PDF writer could not decode either; an image over the
/// pixel limit fails the SVG, as it would on its own.
fn raster(data: &Arc<Vec<u8>>, meter: &Meter) -> Result<bool, Reject> {
    // `probe` would parse an SVG.
    if matches!(sniff(data), None | Some(ImageFormat::Svg)) {
        return Ok(false);
    }
    let info = match super::probe(data) {
        Ok(info) => info,
        Err(super::ProbeError::TooLarge(e)) => {
            return Err(Reject::Limit(format!("SVG image: {e}")));
        }
        Err(super::ProbeError::Invalid(_)) => return Ok(false),
    };
    meter.charge_raster(data, u64::from(info.width) * u64::from(info.height))?;
    Ok(true)
}

#[cfg(test)]
mod tests;
