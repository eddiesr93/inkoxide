//! Colour glyphs drawn as images. After shaping, the glyphs that text
//! uses from colour fonts are turned into images once per render
//! ([`ColorGlyphs::load`]); the painter then draws each as an image in
//! place of the glyph ([`ColorGlyphs::substitute`]), inside a span whose
//! `ActualText` is the glyph's text, with an invisible space as wide as
//! the glyph so text extraction and selection still find it.
//!
//! A glyph whose colour data is missing, malformed or over the limits
//! draws as text when its font has outlines, and not at all otherwise.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use rayon::prelude::*;

use super::UnicodeMap;
use crate::fonts::{ColorGlyph, FontKey, FontSet};
use crate::images::{DecodedImage, ImageCrateDecoder, ImageData, ImageKey, ImageSet};
use crate::paint::{span, InvisibleGlyph, Op, Rect, TextOp, NO_RADII};
use crate::schema::{ImageFit, ObjectPosition};

/// Most distinct colour glyphs one render draws in colour; the others draw
/// as glyphs without colour (in glyph order, by font).
pub(crate) const MAX_COLOR_GLYPHS: usize = 4096;

/// A colour glyph's image and the box it fills, in font units, y up.
struct Placed {
    key: ImageKey,
    bbox: [f32; 4],
}

/// The colour glyphs of one render.
#[derive(Default)]
pub(crate) struct ColorGlyphs {
    glyphs: HashMap<(FontKey, u16), Placed>,
    /// Fonts with a colour glyph here.
    fonts: BTreeSet<FontKey>,
    /// The font whose space spans carry for text extraction: one with
    /// outlines, embedded anyway in most documents.
    space: Option<FontKey>,
}

impl ColorGlyphs {
    /// Turns the glyphs of colour fonts in `used` into images, added to
    /// `images`. On the current rayon pool when `parallel`. With `opaque`
    /// (PDF/A-1b, which forbids transparency), a glyph whose image needs
    /// transparency (an alpha channel, an opacity) has no colour.
    pub fn load(
        fonts: &FontSet<'_>,
        used: &BTreeMap<FontKey, BTreeSet<u16>>,
        images: &mut ImageSet,
        (parallel, opaque): (bool, bool),
    ) -> Self {
        let wanted: Vec<(FontKey, u16)> = used
            .iter()
            .filter(|(&k, _)| fonts.get(k).data.metrics.color)
            .flat_map(|(&k, glyphs)| glyphs.iter().map(move |&g| (k, g)))
            .take(MAX_COLOR_GLYPHS)
            .collect();
        if wanted.is_empty() {
            return Self::default();
        }
        let decode = |&(font, gid): &(FontKey, u16)| -> Option<(Arc<DecodedImage>, [f32; 4])> {
            let (bytes, bbox) = match fonts.get(font).color_glyph(gid)? {
                ColorGlyph::Svg { svg, bbox } => (svg.into_bytes(), bbox),
                ColorGlyph::Png { png, bbox } => (png, bbox),
            };
            // The checks of an image in a document: its header first (a
            // bitmap's size), then the decode, through the process-wide
            // cache, with the SVG limits.
            crate::images::probe(&bytes).ok()?;
            let image = ImageData::new(bytes.into())
                .decoded(&ImageCrateDecoder)
                .ok()?;
            if opaque && crate::pdf::image_transparency(&image).is_some() {
                return None;
            }
            Some((image, bbox))
        };
        let decoded: Vec<_> = if parallel {
            wanted.par_iter().map(decode).collect()
        } else {
            wanted.iter().map(decode).collect()
        };
        let mut out = Self::default();
        for (&(font, gid), image) in wanted.iter().zip(decoded) {
            let Some((image, bbox)) = image else {
                continue;
            };
            let key = images.add(image);
            out.glyphs.insert((font, gid), Placed { key, bbox });
            out.fonts.insert(font);
        }
        let space = |f: FontKey| InvisibleGlyph::space(fonts, f, 1.0, (0.0, 0.0), 1.0).is_some();
        let keys = 0..fonts.fonts.len() as FontKey;
        out.space = keys
            .clone()
            .find(|&k| !fonts.get(k).data.metrics.color && space(k))
            .or_else(|| {
                keys.clone()
                    .find(|&k| !fonts.get(k).data.metrics.color_only && space(k))
            });
        out
    }

    /// Replaces the colour glyphs of the text ops in `ops[from..]` with
    /// their images, each in a span with its text from `unicode`.
    pub fn substitute(
        &self,
        ops: &mut Vec<Op>,
        from: usize,
        fonts: &FontSet<'_>,
        images: &ImageSet,
        unicode: &UnicodeMap,
    ) {
        let colored = |op: &Op| matches!(op, Op::Text(t) if self.fonts.contains(&t.font));
        if !ops[from..].iter().any(colored) {
            return;
        }
        let painted: Vec<Op> = ops.drain(from..).collect();
        for op in painted {
            match op {
                Op::Text(t) if self.fonts.contains(&t.font) => {
                    self.expand(&t, ops, fonts, images, unicode);
                }
                op => ops.push(op),
            }
        }
    }

    /// The colour glyphs of `t` as images, and its other glyphs as text.
    fn expand(
        &self,
        t: &TextOp,
        ops: &mut Vec<Op>,
        fonts: &FontSet<'_>,
        images: &ImageSet,
        unicode: &UnicodeMap,
    ) {
        let font = fonts.get(t.font);
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
            let draw = |ops: &mut Vec<Op>| {
                let fit = ImageFit::Fill;
                images.paint_in(p.key, rect, fit, ObjectPosition::default(), &NO_RADII, ops);
            };
            // A glyph that carries no text (the rest of a cluster) is only
            // drawn.
            match texts.and_then(|m| m.get(&g.id)) {
                Some(text) => {
                    let width = font.advance(g.id) * scale;
                    let space = self
                        .space
                        .and_then(|k| InvisibleGlyph::space(fonts, k, t.size, (x, y), width));
                    span(ops, text, space, draw);
                }
                None => draw(ops),
            }
        }
        // A font without outlines has nothing to draw them with.
        if !plain.is_empty() && !font.data.metrics.color_only {
            ops.push(Op::Text(TextOp {
                glyphs: plain,
                ..*t
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every colour glyph of the test fonts becomes an image: their SVG
    /// passes the SVG limits, their PNG the image checks.
    #[test]
    fn every_colour_glyph_of_the_test_fonts_decodes() {
        let mut store = crate::FontStore::new();
        let ids = ["EmojiTest-COLR", "EmojiTest-CBDT", "EmojiTest-sbix"];
        for id in ids {
            let bytes = crate::test_fixtures::fixture(&format!("fonts/{id}.ttf"));
            store.register(id, bytes).unwrap();
        }
        let json = r#"{"version":1,"fonts":[{"id":"EmojiTest-COLR","family":"a"},
            {"id":"EmojiTest-CBDT","family":"b"},{"id":"EmojiTest-sbix","family":"c"}],"pages":[]}"#;
        let doc: crate::schema::Document = serde_json::from_str(json).unwrap();
        let fonts = FontSet::load(&doc, &store).unwrap();
        for (key, want) in [(0, 10), (1, 4), (2, 4)] {
            let used = BTreeMap::from([(key, (0..64).collect::<BTreeSet<u16>>())]);
            let mut images =
                ImageSet::load(&[], &crate::ImageStore::new(), &ImageCrateDecoder).unwrap();
            let colors = ColorGlyphs::load(&fonts, &used, &mut images, (false, false));
            assert_eq!(colors.glyphs.len(), want, "{}", ids[usize::from(key)]);
            assert_eq!(images.images.len(), want);
            // No font here has outlines and no colour: the spans carry
            // the COLR font's space, which has outlines.
            assert_eq!(colors.space, Some(0));
        }
    }
}
