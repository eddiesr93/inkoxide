//! What PDF/A-1b refuses in a document once its text is shaped: colour
//! glyphs whose images need transparency.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::Cache;
use crate::fonts::{FontKey, FontSet};
use crate::images::{ImageBody, ImageKey, ImageSet};
use crate::pdf::{image_transparency, Profile};
use crate::schema::{Display, Document, ImageSource, Node, NodeKind};
use crate::text::ColorGlyphs;
use crate::ValidationError;

/// The first run, in document order, that draws a colour glyph needing
/// transparency, as the error of a render that forbids it.
pub(super) fn transparent_color_glyph(
    doc: &Document,
    fonts: &FontSet<'_>,
    text: &Cache<'_, '_, '_>,
    images: &ImageSet,
    colors: &ColorGlyphs,
    profile: Profile,
) -> Option<ValidationError> {
    let mut seen: HashMap<(FontKey, u16), Option<&'static str>> = HashMap::new();
    find_shown(doc, |node, path| {
        for (run, font, gid) in text.run_glyphs(node) {
            let need = *seen.entry((font, gid)).or_insert_with(|| {
                let image = colors.image(font, gid, images)?;
                image_transparency(image, profile)
            });
            let Some(need) = need else { continue };
            let glyph =
                (text.unicode().text(font, gid)).map_or(String::new(), |t| format!(" `{t}`"));
            let what = format!(
                "the colour glyph{glyph} of font `{}` ({need})",
                fonts.get(font).id
            );
            let path = format!("{path}.runs[{run}]");
            return Some(crate::validate::transparency_conflict(path, &what));
        }
        None
    })
}

/// The first SVG image, in document order, whose text draws a colour
/// glyph needing transparency, as the error of a render that forbids it.
pub(super) fn transparent_svg_color_glyph(
    doc: &Document,
    fonts: &FontSet<'_>,
    images: &ImageSet,
    colors: &ColorGlyphs,
    profile: Profile,
) -> Option<ValidationError> {
    let mut seen: HashMap<(FontKey, u16), Option<&'static str>> = HashMap::new();
    let mut need = |key: ImageKey| -> Option<String> {
        let ImageBody::Svg {
            probed: Some(probed),
            ..
        } = &images.get(key).body
        else {
            return None;
        };
        let glyphs = probed.texts.iter().flat_map(|t| &t.glyphs);
        for g in glyphs {
            let font = fonts.get(g.font);
            let need = *seen.entry((g.font, g.id)).or_insert_with(|| {
                let image = colors.image(g.font, g.id, images)?;
                image_transparency(image, profile)
            });
            if let Some(need) = need {
                return Some(format!(
                    "a colour glyph of font `{}` in the text of the SVG image ({need})",
                    font.id
                ));
            }
        }
        None
    };
    let found = find_shown(doc, |node, path| {
        let image = images.key(node).map(|k| {
            // An inline image's source is its `data`, as validation names it.
            let field = match &node.kind {
                NodeKind::Image {
                    source: ImageSource::Inline(_),
                    ..
                } => "data",
                _ => "src",
            };
            (k, format!("{path}.{field}"))
        });
        let background = images
            .background_key(node)
            .map(|k| (k, format!("{path}.backgroundImage")));
        let mut runs = Vec::new();
        if let NodeKind::Text { runs: r } = &node.kind {
            for (i, run) in r.iter().enumerate() {
                if let Some(k) = images.run_image_key(run) {
                    runs.push((k, format!("{path}.runs[{i}].image")));
                }
            }
        }
        image
            .into_iter()
            .chain(background)
            .chain(runs)
            .find_map(|(k, path)| Some((path, need(k)?)))
    });
    let (path, what) = found.or_else(|| {
        let emoji = images.emoji_image_keys();
        emoji
            .into_iter()
            .find_map(|k| Some(("emojiImages".to_string(), need(k)?)))
    })?;
    Some(crate::validate::transparency_conflict(path, &what))
}

/// Calls `f` on every displayed node of `doc` with its path
/// (`pages[0].children[2]`), in document order: each page's header,
/// footer, then children, each before its descendants. Stops at, and
/// returns, the first `Some`.
pub(super) fn find_shown<'d, R>(
    doc: &'d Document,
    mut f: impl FnMut(&'d Node, &str) -> Option<R>,
) -> Option<R> {
    fn walk<'d, R>(
        node: &'d Node,
        path: &mut String,
        f: &mut dyn FnMut(&'d Node, &str) -> Option<R>,
    ) -> Option<R> {
        if node.hidden() {
            return None;
        }
        if let Some(r) = f(node, path) {
            return Some(r);
        }
        let len = path.len();
        let mut found = None;
        match &node.kind {
            NodeKind::Box { children, .. } => {
                for (i, c) in children.iter().enumerate() {
                    let _ = write!(path, ".children[{i}]");
                    found = walk(c, path, f);
                    path.truncate(len);
                    if found.is_some() {
                        break;
                    }
                }
            }
            NodeKind::Table(t) => {
                let cells = t.rows.iter().enumerate().flat_map(|(r, row)| {
                    row.cells
                        .iter()
                        .enumerate()
                        .map(move |(c, cell)| (r, c, cell))
                });
                'cells: for (r, c, cell) in cells {
                    if cell.style.display == Some(Display::None) {
                        continue;
                    }
                    for (i, child) in cell.children.iter().enumerate() {
                        let _ = write!(path, ".rows[{r}].cells[{c}].children[{i}]");
                        found = walk(child, path, f);
                        path.truncate(len);
                        if found.is_some() {
                            break 'cells;
                        }
                    }
                }
            }
            _ => {}
        }
        found
    }
    let mut path = String::new();
    for (i, page) in doc.pages.iter().enumerate() {
        let fixed = [("header", &page.header), ("footer", &page.footer)];
        let fixed = fixed
            .into_iter()
            .filter_map(|(name, n)| Some((name.to_string(), n.as_deref()?)));
        let children = page
            .children
            .iter()
            .enumerate()
            .map(|(k, n)| (format!("children[{k}]"), n));
        for (key, node) in fixed.chain(children) {
            path.clear();
            let _ = write!(path, "pages[{i}].{key}");
            if let Some(r) = walk(node, &mut path, &mut f) {
                return Some(r);
            }
        }
    }
    None
}
