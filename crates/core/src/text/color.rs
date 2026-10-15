//! Colour glyphs drawn as images. After shaping, the glyphs that text
//! uses from colour fonts are turned into images once per render
//! ([`ColorGlyphs::load`]); the painter then draws each as an image in
//! place of the glyph ([`ColorGlyphs::push`]), inside a span whose
//! `ActualText` is the glyph's text, with an invisible space as wide as
//! the glyph (of the run's font, or another with outlines) so text
//! extraction and selection still find it.
//!
//! A glyph whose colour data is missing, malformed or over the limits
//! draws as text when its font has outlines, and not at all otherwise.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use rayon::prelude::*;

use super::UnicodeMap;
use crate::fonts::{ColorGlyph, FontKey, FontSet, LoadedFont};
use crate::images::{DecodedImage, ImageCrateDecoder, ImageData, ImageKey, ImageSet, Placement};
use crate::paint::{InvisibleGlyph, Op, Rect, TextOp};

/// Most distinct colour glyphs one render draws in colour; the others draw
/// as glyphs without colour (in glyph order, by font).
pub(crate) const MAX_COLOR_GLYPHS: usize = 4096;

/// Most bytes of colour glyph images (`COLR` glyphs written as SVG, PNG
/// bitmaps) one render makes and keeps; past them, the other glyphs draw
/// without colour (in glyph order, by font). Each glyph is bounded on its
/// own too (`fonts::color`), and emoji take a few kilobytes each.
pub(crate) const MAX_COLOR_BYTES: usize = 32 << 20;

/// How many glyphs are turned into images at once: what one step holds
/// before the byte budget is counted.
const BATCH: usize = 64;

/// A colour glyph's image and the box it fills, in font units, y up.
struct Placed {
    key: ImageKey,
    bbox: [f32; 4],
}

/// The colour glyphs of one render.
#[derive(Default)]
pub(crate) struct ColorGlyphs {
    glyphs: HashMap<(FontKey, u16), Placed>,
}

/// The image bytes of glyph `gid` of `font` (SVG or PNG), and the box they
/// fill. `None` for a glyph without colour, or whose colour data is
/// malformed or over the limits of one glyph.
fn glyph_bytes(font: &LoadedFont<'_>, gid: u16) -> Option<(Vec<u8>, [f32; 4])> {
    Some(match font.color_glyph(gid)? {
        ColorGlyph::Svg { svg, bbox } => (svg.into_bytes(), bbox),
        ColorGlyph::Png { png, bbox } => (png, bbox),
    })
}

/// A colour glyph's image, through the checks of an image in a document:
/// its header first (a bitmap's size), then the decode, through the
/// process-wide cache, with the SVG limits.
fn decode(bytes: Vec<u8>) -> Option<Arc<DecodedImage>> {
    crate::images::probe(&bytes).ok()?;
    ImageData::new(bytes.into())
        .decoded(&ImageCrateDecoder)
        .ok()
}

/// What colour glyph `gid` of `font` needs transparency for (an alpha
/// channel, an opacity), which PDF/A-1b forbids; `None` when it draws
/// opaque, or has no colour.
pub(crate) fn glyph_transparency(font: &LoadedFont<'_>, gid: u16) -> Option<&'static str> {
    let (bytes, _) = glyph_bytes(font, gid)?;
    crate::pdf::image_transparency(&*decode(bytes)?)
}

/// The limits of [`ColorGlyphs::load`]: [`MAX_COLOR_GLYPHS`] and
/// [`MAX_COLOR_BYTES`], smaller in tests.
#[derive(Clone, Copy)]
struct Budget {
    glyphs: usize,
    bytes: usize,
}

impl ColorGlyphs {
    /// Turns the glyphs of colour fonts in `used` into images, added to
    /// `images`. On the current rayon pool when `parallel`.
    pub fn load(
        fonts: &FontSet<'_>,
        used: &BTreeMap<FontKey, BTreeSet<u16>>,
        images: &mut ImageSet,
        parallel: bool,
    ) -> Self {
        let budget = Budget {
            glyphs: MAX_COLOR_GLYPHS,
            bytes: MAX_COLOR_BYTES,
        };
        Self::load_within(fonts, used, images, parallel, budget)
    }

    fn load_within(
        fonts: &FontSet<'_>,
        used: &BTreeMap<FontKey, BTreeSet<u16>>,
        images: &mut ImageSet,
        parallel: bool,
        budget: Budget,
    ) -> Self {
        let wanted: Vec<(FontKey, u16)> = used
            .iter()
            .filter(|(&k, _)| fonts.get(k).has_color())
            .flat_map(|(&k, glyphs)| glyphs.iter().map(move |&g| (k, g)))
            .take(budget.glyphs)
            .collect();
        let mut out = Self::default();
        let mut bytes_left = budget.bytes;
        // In batches, so that at most one batch of images is held before
        // the budget is counted, in glyph order whatever the threads.
        for batch in wanted.chunks(BATCH) {
            let make = |&(font, gid): &(FontKey, u16)| glyph_bytes(fonts.get(font), gid);
            let made: Vec<_> = if parallel {
                batch.par_iter().map(make).collect()
            } else {
                batch.iter().map(make).collect()
            };
            let mut kept = Vec::with_capacity(batch.len());
            let mut over = false;
            for (&glyph, made) in batch.iter().zip(made) {
                let Some((bytes, bbox)) = made else { continue };
                if bytes.len() > bytes_left {
                    over = true;
                    break;
                }
                bytes_left -= bytes.len();
                kept.push((glyph, bytes, bbox));
            }
            let decoded: Vec<_> = if parallel {
                kept.into_par_iter()
                    .map(|(g, bytes, bbox)| (g, decode(bytes), bbox))
                    .collect()
            } else {
                kept.into_iter()
                    .map(|(g, bytes, bbox)| (g, decode(bytes), bbox))
                    .collect()
            };
            for (glyph, image, bbox) in decoded {
                if let Some(image) = image {
                    let key = images.add(image);
                    out.glyphs.insert(glyph, Placed { key, bbox });
                }
            }
            if over {
                break;
            }
        }
        out
    }

    /// Whether glyph `gid` of `font` is drawn in colour.
    pub fn draws(&self, font: FontKey, gid: u16) -> bool {
        self.glyphs.contains_key(&(font, gid))
    }

    /// Pushes text op `t`: as it is, or, in a colour font, with its colour
    /// glyphs as images, each in a span with its text from `unicode` and
    /// an invisible space of font `strut` as wide as the glyph. The other
    /// glyphs of a font without outlines are left out: it has nothing to
    /// draw them with.
    pub fn push(
        &self,
        t: TextOp,
        strut: Option<FontKey>,
        fonts: &FontSet<'_>,
        images: &ImageSet,
        unicode: &UnicodeMap,
        ops: &mut Vec<Op>,
    ) {
        let font = fonts.get(t.font);
        if !font.has_color() {
            ops.push(Op::Text(t));
            return;
        }
        let scale = font.scale(t.size);
        let texts = unicode.fonts.get(&t.font);
        let mut plain = Vec::new();
        for g in &t.glyphs {
            let Some(p) = self.glyphs.get(&(t.font, g.id)) else {
                plain.push(*g);
                continue;
            };
            let (x, y) = (t.x + g.x, t.y - g.y_offset);
            let [x0, y0, x1, y1] = p.bbox;
            let rect = Rect::new(
                x + x0 * scale,
                y - y1 * scale,
                (x1 - x0) * scale,
                (y1 - y0) * scale,
            );
            // A glyph that carries no text (the rest of a cluster) is only
            // drawn.
            let text = texts.and_then(|m| m.get(&g.id)).map(|text| {
                let width = font.advance(g.id) * scale;
                let space =
                    strut.and_then(|k| InvisibleGlyph::space(fonts, k, t.size, (x, y), width));
                (text.as_str(), space)
            });
            super::paint::image_span(images, p.key, rect, Placement::FILL, text, ops);
        }
        if !plain.is_empty() && font.has_outlines() {
            ops.push(Op::Text(TextOp { glyphs: plain, ..t }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour test fonts, keyed 0 to 2: `EmojiTest-COLR`, `-CBDT` and
    /// `-sbix`.
    fn with_fonts(f: impl FnOnce(&FontSet<'_>)) {
        let mut store = crate::FontStore::new();
        for id in ["EmojiTest-COLR", "EmojiTest-CBDT", "EmojiTest-sbix"] {
            let bytes = crate::test_fixtures::fixture(&format!("fonts/{id}.ttf"));
            store.register(id, bytes).unwrap();
        }
        let json = r#"{"version":1,"fonts":[{"id":"EmojiTest-COLR","family":"a"},
            {"id":"EmojiTest-CBDT","family":"b"},{"id":"EmojiTest-sbix","family":"c"}],"pages":[]}"#;
        let doc: crate::schema::Document = serde_json::from_str(json).unwrap();
        f(&FontSet::load(&doc, &store).unwrap());
    }

    fn no_images() -> ImageSet {
        ImageSet::load(&[], &crate::ImageStore::new(), &ImageCrateDecoder).unwrap()
    }

    /// The colour glyphs loaded, in glyph order.
    fn loaded(colors: &ColorGlyphs) -> Vec<(FontKey, u16)> {
        let mut glyphs: Vec<_> = colors.glyphs.keys().copied().collect();
        glyphs.sort_unstable();
        glyphs
    }

    /// Every colour glyph of the test fonts becomes an image: their SVG
    /// passes the SVG limits, their PNG the image checks.
    #[test]
    fn every_colour_glyph_of_the_test_fonts_decodes() {
        with_fonts(|fonts| {
            for (key, want) in [(0, 10), (1, 4), (2, 4)] {
                let used = BTreeMap::from([(key, (0..64).collect::<BTreeSet<u16>>())]);
                let mut images = no_images();
                let colors = ColorGlyphs::load(fonts, &used, &mut images, false);
                assert_eq!(colors.glyphs.len(), want, "{}", fonts.get(key).id);
                assert_eq!(images.images.len(), want);
            }
        });
    }

    /// Past [`MAX_COLOR_GLYPHS`] glyphs of colour fonts, in glyph order by
    /// font, the others have no colour.
    #[test]
    fn colour_glyphs_past_the_most_of_a_render_have_no_colour() {
        with_fonts(|fonts| {
            let all = |n: usize| (0..n as u16).collect::<BTreeSet<u16>>();
            let used = BTreeMap::from([(0, all(MAX_COLOR_GLYPHS - 1)), (1, all(64))]);
            let colors = ColorGlyphs::load(fonts, &used, &mut no_images(), false);
            let glyphs = loaded(&colors);
            // The COLR font's ten, and the CBDT font's glyph 0, which has
            // no colour.
            assert_eq!(glyphs.len(), 10, "{glyphs:?}");
            assert!(glyphs.iter().all(|g| g.0 == 0));
            let used = BTreeMap::from([(0, all(MAX_COLOR_GLYPHS - 64)), (1, all(64))]);
            let colors = ColorGlyphs::load(fonts, &used, &mut no_images(), false);
            assert_eq!(loaded(&colors).len(), 14);
        });
    }

    /// Past [`MAX_COLOR_BYTES`] of images, the other glyphs, in glyph
    /// order, have no colour, whatever the threads.
    #[test]
    fn colour_glyphs_past_the_byte_budget_have_no_colour() {
        with_fonts(|fonts| {
            let used = BTreeMap::from([(0, (0..64).collect::<BTreeSet<u16>>())]);
            let all = ColorGlyphs::load(fonts, &used, &mut no_images(), false);
            let glyphs = loaded(&all);
            let size = |&(f, g): &(FontKey, u16)| glyph_bytes(fonts.get(f), g).unwrap().0.len();
            let three: usize = glyphs[..3].iter().map(size).sum();
            for parallel in [false, true] {
                for (bytes, want) in [(three, 3), (three + size(&glyphs[3]) - 1, 3), (0, 0)] {
                    let budget = Budget {
                        glyphs: MAX_COLOR_GLYPHS,
                        bytes,
                    };
                    let mut images = no_images();
                    let colors =
                        ColorGlyphs::load_within(fonts, &used, &mut images, parallel, budget);
                    assert_eq!(loaded(&colors), glyphs[..want], "{bytes} {parallel}");
                    assert_eq!(images.images.len(), want);
                }
            }
        });
    }

    #[test]
    fn transparency_of_colour_glyphs() {
        with_fonts(|fonts| {
            let (colr, cbdt) = (fonts.get(0), fonts.get(1));
            let glyph = |f: &LoadedFont<'_>, c: char| f.glyph(c).unwrap();
            assert_eq!(
                glyph_transparency(cbdt, glyph(cbdt, '😀')),
                Some("an alpha channel")
            );
            assert!(glyph_transparency(colr, glyph(colr, '😀')).is_some());
            // Solid layers only.
            assert_eq!(glyph_transparency(colr, glyph(colr, '\u{2764}')), None);
            // No colour.
            assert_eq!(glyph_transparency(colr, glyph(colr, ' ')), None);
        });
    }
}
