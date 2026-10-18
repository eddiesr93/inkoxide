//! Colour glyphs drawn as images. After shaping, the glyphs that text
//! uses from colour fonts are turned into images once per render
//! ([`ColorGlyphs::load`]), and each image is a glyph of a Type 3 font
//! (256 to a font, in glyph order) whose procedure draws it. The page
//! painter draws a colour glyph as that glyph ([`ColorGlyphs::push`]),
//! inside a span whose `ActualText` is its text, so that text extraction
//! and selection find it as text. In SVG text, which draws in a form of its
//! own, a colour glyph is the image itself, in such a span with an
//! invisible space as wide as the glyph (of the run's font, or another
//! with outlines).
//!
//! A `COLR` glyph with layers in the foreground colour takes the text
//! colour: it becomes one image per text colour it is drawn in (the
//! colour's alpha is the opacity the painter draws the run in).
//!
//! A glyph whose colour data is missing, malformed or over the limits
//! draws as text when its font has outlines. Otherwise nothing draws it,
//! but it keeps its text: a span with its `ActualText` and an invisible
//! space as wide as the glyph, as a drawn colour glyph has.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use rayon::prelude::*;

use crate::fonts::{ColorGlyph, FontKey, FontSet, LoadedFont, TextColor, BLACK_TEXT};
use crate::geom::Rect;
use crate::images::{
    DecodedImage, ImageCrateDecoder, ImageData, ImageKey, ImagePlacement, ImageSet,
};
use crate::paint::{ColorGlyphOp, InvisibleGlyph, Op, TextOp};

/// Most distinct colour glyphs one render draws in colour; the others draw
/// as glyphs without colour (in glyph order, by font). Glyphs of colour
/// fonts that have no colour data do not count; one that takes the text
/// colour counts once per text colour it is drawn in.
pub(crate) const MAX_COLOR_GLYPHS: usize = 4096;

/// Most bytes of colour glyph images (`COLR` glyphs written as SVG, PNG
/// bitmaps) one render makes and keeps; past them, the other glyphs draw
/// without colour (in glyph order, by font). Each glyph is bounded on its
/// own too (`fonts::color`), and emoji take a few kilobytes each.
pub(crate) const MAX_COLOR_BYTES: usize = 32 << 20;

/// How many glyphs are turned into images at once: what one step holds
/// before the byte budget is counted.
const BATCH: usize = 64;

/// The glyphs of colour fonts that text uses, by font then glyph, each
/// with the text colours it is drawn in.
pub(crate) type UsedGlyphs = BTreeMap<(FontKey, u16), BTreeSet<TextColor>>;

/// The text colour of colour `c` (`[r, g, b, a]`, 0 to 1), its alpha left
/// to the opacity the text is drawn in.
pub(crate) fn text_color(c: [f32; 4]) -> TextColor {
    [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A colour glyph's image, the box it fills, in font units, y up, and its
/// Type 3 font and code.
struct Placed {
    key: ImageKey,
    bbox: [f32; 4],
    slot: (u16, u8),
}

/// Most glyphs of a Type 3 font: its codes are bytes, and code 0 is none
/// of them (`pdf::type3`).
const TYPE3_GLYPHS: usize = 255;

/// A Type 3 font of colour glyphs: images of glyphs of `font`, the procedure
/// of code `i + 1` drawing `glyphs[i]`.
pub(crate) struct Type3Font {
    pub font: FontKey,
    pub glyphs: Vec<Type3Glyph>,
}

/// The glyph of a Type 3 font: an image, the box it fills and the
/// advance, in font units, y up.
pub(crate) struct Type3Glyph {
    pub image: ImageKey,
    pub bbox: [f32; 4],
    pub advance: f32,
}

/// The images of a colour glyph: one, or one per text colour for a glyph
/// that takes the text colour.
enum Images {
    One(Placed),
    ByColor(HashMap<TextColor, Placed>),
}

/// The colour glyphs of one render.
#[derive(Default)]
pub(crate) struct ColorGlyphs {
    glyphs: HashMap<(FontKey, u16), Images>,
    type3: Vec<Type3Font>,
}

/// The image bytes of colour glyph `glyph` (SVG or PNG), and the box they
/// fill.
fn bytes_of(glyph: ColorGlyph) -> (Vec<u8>, [f32; 4]) {
    match glyph {
        ColorGlyph::Svg { svg, bbox } => (svg.into_bytes(), bbox),
        ColorGlyph::Png { png, bbox } => (png, bbox),
    }
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

/// The limits of [`ColorGlyphs::load`]: [`MAX_COLOR_GLYPHS`] and
/// [`MAX_COLOR_BYTES`], smaller in tests.
#[derive(Clone, Copy)]
struct Budget {
    glyphs: usize,
    bytes: usize,
}

/// An image made for a colour glyph: its text colour (for a glyph that
/// takes it), its bytes, and the box they fill.
type Made = (Option<TextColor>, Vec<u8>, [f32; 4]);

/// The images of glyph `gid` of `font`, which takes the text colour, one
/// per text colour in `colors` (`black`: painted in black), with their
/// colour and the box they fill; `None` when they do not all fit in
/// `left`. Painted in batches, on the current rayon pool when `parallel`,
/// so that at most a batch is held past the byte budget.
fn tints(
    font: &LoadedFont<'_>,
    gid: u16,
    black: ColorGlyph,
    colors: &BTreeSet<TextColor>,
    left: Budget,
    parallel: bool,
) -> Option<Vec<Made>> {
    if colors.len() > left.glyphs {
        return None;
    }
    let colors: Vec<TextColor> = colors.iter().copied().collect();
    let mut black = Some(black);
    let mut out = Vec::with_capacity(colors.len());
    let mut bytes = 0;
    for batch in colors.chunks(BATCH) {
        let paint = |&c: &TextColor| (c, font.color_glyph_in(gid, c).map(bytes_of));
        let painted: Vec<_> = if parallel {
            batch
                .par_iter()
                .filter(|&&c| c != BLACK_TEXT)
                .map(paint)
                .collect()
        } else {
            batch
                .iter()
                .filter(|&&c| c != BLACK_TEXT)
                .map(paint)
                .collect()
        };
        let black = batch
            .contains(&BLACK_TEXT)
            .then(|| black.take())
            .flatten()
            .map(|b| (BLACK_TEXT, Some(bytes_of(b))));
        for (c, made) in black.into_iter().chain(painted) {
            // In a colour whose markup is over the limits, it has no
            // colour.
            let Some((b, bbox)) = made else { continue };
            bytes += b.len();
            if bytes > left.bytes {
                return None;
            }
            out.push((Some(c), b, bbox));
        }
    }
    Some(out)
}

impl ColorGlyphs {
    /// Turns the glyphs of colour fonts in `used` into images, added to
    /// `images`: one per glyph, or one per text colour for a glyph that
    /// takes the text colour. On the current rayon pool when `parallel`.
    pub fn load(
        fonts: &FontSet<'_>,
        used: &UsedGlyphs,
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
        used: &UsedGlyphs,
        images: &mut ImageSet,
        parallel: bool,
        budget: Budget,
    ) -> Self {
        let wanted: Vec<(&(FontKey, u16), &BTreeSet<TextColor>)> = used
            .iter()
            .filter(|((k, _), _)| fonts.get(*k).has_color())
            .collect();
        let mut out = Self::default();
        let mut left = budget;
        // In batches, so that at most one batch of images is held before
        // the budget is counted, in glyph order whatever the threads.
        for batch in wanted.chunks(BATCH) {
            let make = |&(&(font, gid), _): &(&(FontKey, u16), &BTreeSet<TextColor>)| {
                fonts.get(font).color_glyph_tinted(gid)
            };
            let made: Vec<_> = if parallel {
                batch.par_iter().map(make).collect()
            } else {
                batch.iter().map(make).collect()
            };
            let mut kept = Vec::with_capacity(batch.len());
            let mut over = false;
            for (&(&glyph, colors), made) in batch.iter().zip(made) {
                // A glyph without colour data is not a colour glyph.
                let Some((black, tinted)) = made else {
                    continue;
                };
                let made = if tinted {
                    tints(fonts.get(glyph.0), glyph.1, black, colors, left, parallel)
                } else {
                    let (bytes, bbox) = bytes_of(black);
                    let fits = left.glyphs > 0 && bytes.len() <= left.bytes;
                    fits.then(|| vec![(None, bytes, bbox)])
                };
                let Some(made) = made else {
                    over = true;
                    break;
                };
                left.glyphs -= made.len();
                left.bytes -= made.iter().map(|m| m.1.len()).sum::<usize>();
                kept.extend(made.into_iter().map(|(c, b, bbox)| (glyph, c, b, bbox)));
            }
            let decode = |(g, c, bytes, bbox)| (g, c, decode(bytes), bbox);
            let decoded: Vec<_> = if parallel {
                kept.into_par_iter().map(decode).collect()
            } else {
                kept.into_iter().map(decode).collect()
            };
            for (glyph, color, image, bbox) in decoded {
                let Some(image) = image else { continue };
                let key = images.add(image);
                let advance = fonts.get(glyph.0).advance(glyph.1);
                let slot = out.slot(
                    glyph.0,
                    Type3Glyph {
                        image: key,
                        bbox,
                        advance,
                    },
                );
                let placed = Placed { key, bbox, slot };
                match color {
                    None => {
                        out.glyphs.insert(glyph, Images::One(placed));
                    }
                    Some(c) => {
                        let entry = out
                            .glyphs
                            .entry(glyph)
                            .or_insert_with(|| Images::ByColor(HashMap::new()));
                        if let Images::ByColor(by) = entry {
                            by.insert(c, placed);
                        }
                    }
                }
            }
            if over {
                break;
            }
        }
        out
    }

    /// Code `glyph` in a Type 3 font of glyphs of `font`: the last one,
    /// or a new one when that one is full or of another font.
    fn slot(&mut self, font: FontKey, glyph: Type3Glyph) -> (u16, u8) {
        let full = self
            .type3
            .last()
            .is_none_or(|t| t.font != font || t.glyphs.len() >= TYPE3_GLYPHS);
        if full {
            self.type3.push(Type3Font {
                font,
                glyphs: Vec::new(),
            });
        }
        let index = self.type3.len() - 1;
        let t = &mut self.type3[index];
        t.glyphs.push(glyph);
        // At most `MAX_COLOR_GLYPHS` glyphs, so at most 17 fonts.
        (index as u16, t.glyphs.len() as u8)
    }

    /// The Type 3 fonts of the colour glyphs, indexed by
    /// [`ColorGlyphOp::font`].
    pub fn type3(&self) -> &[Type3Font] {
        &self.type3
    }

    /// Whether glyph `gid` of `font` is drawn in colour.
    pub fn draws(&self, font: FontKey, gid: u16) -> bool {
        self.glyphs.contains_key(&(font, gid))
    }

    /// The image glyph `gid` of `font` is drawn as (for a glyph that takes
    /// the text colour, in the first of its colours); `None` when it is
    /// not drawn in colour.
    pub fn image<'i>(
        &self,
        font: FontKey,
        gid: u16,
        images: &'i ImageSet,
    ) -> Option<&'i DecodedImage> {
        let placed = match self.glyphs.get(&(font, gid))? {
            Images::One(p) => p,
            Images::ByColor(by) => by.iter().min_by_key(|(c, _)| **c)?.1,
        };
        Some(images.get(placed.key))
    }

    /// The image of glyph `gid` of `font` in text colour `color`.
    fn placed(&self, font: FontKey, gid: u16, color: TextColor) -> Option<&Placed> {
        match self.glyphs.get(&(font, gid))? {
            Images::One(p) => Some(p),
            Images::ByColor(by) => by.get(&color),
        }
    }

    /// Pushes text op `t`: as it is, or, in a colour font, with its colour
    /// glyphs each in a span with its text from `texts` (one for each
    /// glyph of `t`; none past its end): as glyphs of their Type 3 fonts
    /// when `type3`, else as images with an invisible space of font
    /// `strut` as wide as the glyph. The other glyphs of a font without
    /// outlines, which has nothing to draw them with, keep only such a
    /// span, with a space and nothing drawn: their text still copies.
    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &self,
        t: TextOp,
        strut: Option<FontKey>,
        fonts: &FontSet<'_>,
        images: &ImageSet,
        texts: &[Option<&str>],
        type3: bool,
        ops: &mut Vec<Op>,
    ) {
        let font = fonts.get(t.font);
        if !font.has_color() {
            ops.push(Op::Text(t));
            return;
        }
        let scale = font.scale(t.size);
        let outlined = font.has_outlines();
        let color = text_color(t.color);
        let mut plain = Vec::new();
        for (i, g) in t.glyphs.iter().enumerate() {
            let (x, y) = (t.x + g.x, t.y - g.y_offset);
            // A glyph that carries no text (the rest of a cluster) is only
            // drawn.
            let text = texts.get(i).copied().flatten().map(|text| {
                let width = font.advance(g.id) * scale;
                let space =
                    strut.and_then(|k| InvisibleGlyph::space(fonts, k, t.size, (x, y), width));
                (text, space)
            });
            let Some(p) = self.placed(t.font, g.id, color) else {
                if outlined {
                    plain.push(*g);
                } else if let Some((text, space)) = text {
                    crate::paint::span(ops, text, space, |_| {});
                }
                continue;
            };
            if type3 {
                let glyph = Op::ColorGlyph(ColorGlyphOp {
                    font: p.slot.0,
                    code: p.slot.1,
                    size: t.size,
                    x,
                    y,
                });
                match text {
                    Some((text, _)) => crate::paint::span(ops, text, None, |ops| ops.push(glyph)),
                    None => ops.push(glyph),
                }
                continue;
            }
            let [x0, y0, x1, y1] = p.bbox;
            let rect = Rect::new(
                x + x0 * scale,
                y - y1 * scale,
                (x1 - x0) * scale,
                (y1 - y0) * scale,
            );
            super::paint::image_span(images, p.key, rect, ImagePlacement::FILL, text, ops);
        }
        if !plain.is_empty() && outlined {
            ops.push(Op::Text(TextOp { glyphs: plain, ..t }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour test fonts, keyed 0 to 3: `EmojiTest-COLR`, `-CBDT`,
    /// `-sbix` and `-SVG`.
    fn with_fonts(f: impl FnOnce(&FontSet<'_>)) {
        let mut store = crate::FontStore::new();
        for id in [
            "EmojiTest-COLR",
            "EmojiTest-CBDT",
            "EmojiTest-sbix",
            "EmojiTest-SVG",
        ] {
            let bytes = crate::test_fixtures::fixture(&format!("fonts/{id}.ttf"));
            store.register(id, bytes).unwrap();
        }
        let json = r#"{"version":1,"fonts":[{"id":"EmojiTest-COLR","family":"a"},
            {"id":"EmojiTest-CBDT","family":"b"},{"id":"EmojiTest-sbix","family":"c"},{"id":"EmojiTest-SVG","family":"d"}],"pages":[]}"#;
        let doc: crate::schema::Document = serde_json::from_str(json).unwrap();
        f(&FontSet::load(&doc, &store).unwrap());
    }

    #[test]
    fn a_variable_colr_glyph_is_painted_at_the_instance() {
        let mut store = crate::FontStore::new();
        let bytes = crate::test_fixtures::fixture("fonts/VarTest-COLRv1.ttf");
        store.register("VarTest-COLRv1", bytes).unwrap();
        let json = r#"{"version":1,"fonts":[{"id":"a","family":"a","source":"VarTest-COLRv1"},
            {"id":"b","family":"b","source":"VarTest-COLRv1","variations":{"wght":900}}],"pages":[]}"#;
        let doc: crate::schema::Document = serde_json::from_str(json).unwrap();
        let fonts = FontSet::load(&doc, &store).unwrap();
        let svg = |key| match fonts.get(key).color_glyph(1) {
            Some(ColorGlyph::Svg { svg, .. }) => svg,
            other => panic!("{other:?}"),
        };
        // The square fades from opaque to half (127 of 255) with wght.
        let (default, bold) = (svg(0), svg(1));
        assert!(!default.contains("fill-opacity"), "{default}");
        assert!(bold.contains(r#"fill-opacity="0.49803922""#), "{bold}");
    }

    /// The image bytes of glyph `gid` of `font` (SVG or PNG), its layers
    /// in the foreground colour black, and the box they fill.
    fn glyph_bytes(font: &LoadedFont<'_>, gid: u16) -> Option<(Vec<u8>, [f32; 4])> {
        font.color_glyph(gid).map(bytes_of)
    }

    fn no_images() -> ImageSet {
        ImageSet::load(&[], &crate::ImageStore::new(), &ImageCrateDecoder).unwrap()
    }

    /// Glyphs `used`, by font, each drawn in black.
    fn black(used: &BTreeMap<FontKey, BTreeSet<u16>>) -> UsedGlyphs {
        let glyphs = used
            .iter()
            .flat_map(|(&k, g)| g.iter().map(move |&g| (k, g)));
        glyphs.map(|g| (g, BTreeSet::from([BLACK_TEXT]))).collect()
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
            for (key, want) in [(0, 12), (1, 4), (2, 4), (3, 5)] {
                let used = BTreeMap::from([(key, (0..64).collect::<BTreeSet<u16>>())]);
                let mut images = no_images();
                let colors = ColorGlyphs::load(fonts, &black(&used), &mut images, false);
                assert_eq!(colors.glyphs.len(), want, "{}", fonts.get(key).id);
                assert_eq!(images.images.len(), want);
            }
        });
    }

    /// Past [`MAX_COLOR_GLYPHS`] colour glyphs, in glyph order by font,
    /// the others have no colour; glyphs without colour do not count.
    #[test]
    fn colour_glyphs_past_the_most_of_a_render_have_no_colour() {
        with_fonts(|fonts| {
            let all = |n: usize| (0..n as u16).collect::<BTreeSet<u16>>();
            // The COLR font's twelve and the CBDT font's four, however many
            // glyphs without colour come first.
            let used = BTreeMap::from([(0, all(MAX_COLOR_GLYPHS - 1)), (1, all(64))]);
            let colors = ColorGlyphs::load(fonts, &black(&used), &mut no_images(), false);
            let glyphs = loaded(&colors);
            assert_eq!(glyphs.len(), 16, "{glyphs:?}");
            for parallel in [false, true] {
                let budget = Budget {
                    glyphs: 12,
                    bytes: MAX_COLOR_BYTES,
                };
                let colors = ColorGlyphs::load_within(
                    fonts,
                    &black(&used),
                    &mut no_images(),
                    parallel,
                    budget,
                );
                assert_eq!(loaded(&colors), glyphs[..12], "{parallel}");
            }
        });
    }

    /// A glyph with layers in the foreground colour has an image per text
    /// colour it is drawn in, each counted against the limit, and is drawn
    /// with the one of its text's colour; the others have one image.
    #[test]
    fn glyphs_in_the_text_colour_have_an_image_per_colour() {
        with_fonts(|fonts| {
            let colr = fonts.get(0);
            let (star, grin) = (colr.glyph('\u{2B50}').unwrap(), colr.glyph('😀').unwrap());
            let (red, blue) = ([255, 0, 0], [0, 0, 255]);
            let used = UsedGlyphs::from([
                ((0, star), BTreeSet::from([red, blue, BLACK_TEXT])),
                ((0, grin), BTreeSet::from([red, blue])),
            ]);
            for parallel in [false, true] {
                let mut images = no_images();
                let colors = ColorGlyphs::load(fonts, &used, &mut images, parallel);
                assert_eq!(images.images.len(), 4);
                let key = |gid, c: [f32; 4]| colors.placed(0, gid, text_color(c)).unwrap().key;
                let (r, b) = ([1.0, 0.0, 0.0, 0.5], [0.0, 0.0, 1.0, 1.0]);
                assert_ne!(key(star, r), key(star, b));
                assert_ne!(key(star, r), key(star, [0.0, 0.0, 0.0, 1.0]));
                assert_eq!(key(grin, r), key(grin, b));
                // A colour it was not loaded in: no image.
                assert!(colors.placed(0, star, [0, 255, 0]).is_none());
                // The star's three colours count three against the limit,
                // all or none, after the grin's one.
                let both = vec![(0, grin), (0, star)];
                for (glyphs, want) in [(4, both), (3, vec![(0, grin)])] {
                    let budget = Budget {
                        glyphs,
                        bytes: MAX_COLOR_BYTES,
                    };
                    let colors =
                        ColorGlyphs::load_within(fonts, &used, &mut no_images(), parallel, budget);
                    assert_eq!(loaded(&colors), want, "{glyphs}");
                }
            }
        });
    }

    /// The glyphs of a font without outlines that are not drawn in colour
    /// (past the limits, or without colour) keep their text: a span with
    /// an invisible space, and nothing drawn.
    #[test]
    fn glyphs_of_a_font_without_outlines_keep_their_text() {
        with_fonts(|fonts| {
            let cbdt = fonts.get(1);
            assert!(!cbdt.has_outlines());
            let ids: Vec<u16> = ['😀', ' ']
                .iter()
                .map(|&c| cbdt.glyph(c).unwrap())
                .collect();
            let text = TextOp {
                font: 1,
                size: 20.0,
                color: [0.0, 0.0, 0.0, 1.0],
                x: 10.0,
                y: 30.0,
                glyphs: ids
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| crate::paint::TextGlyph {
                        id,
                        x: 20.0 * i as f32,
                        y_offset: 0.0,
                        continues: false,
                    })
                    .collect(),
            };
            // No colour glyphs: as past the limits. The COLR font has
            // outlines and a space.
            let mut ops = Vec::new();
            let texts = [Some("😀"), Some(" ")];
            let images = no_images();
            ColorGlyphs::default().push(text, Some(0), fonts, &images, &texts, true, &mut ops);
            let spans: Vec<&str> = ops
                .iter()
                .filter_map(|op| match op {
                    Op::Span { actual_text } => Some(&**actual_text),
                    _ => None,
                })
                .collect();
            assert_eq!(spans, ["😀", " "]);
            let spaces = ops
                .iter()
                .filter(|op| matches!(op, Op::InvisibleText(g) if g.font == 0))
                .count();
            assert_eq!(spaces, 2);
            assert!(
                !ops.iter()
                    .any(|op| matches!(op, Op::Image { .. } | Op::Text(_))),
                "{ops:?}"
            );
        });
    }

    /// Past [`MAX_COLOR_BYTES`] of images, the other glyphs, in glyph
    /// order, have no colour, whatever the threads.
    #[test]
    fn colour_glyphs_past_the_byte_budget_have_no_colour() {
        with_fonts(|fonts| {
            let used = BTreeMap::from([(0, (0..64).collect::<BTreeSet<u16>>())]);
            let all = ColorGlyphs::load(fonts, &black(&used), &mut no_images(), false);
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
                    let colors = ColorGlyphs::load_within(
                        fonts,
                        &black(&used),
                        &mut images,
                        parallel,
                        budget,
                    );
                    assert_eq!(loaded(&colors), glyphs[..want], "{bytes} {parallel}");
                    assert_eq!(images.images.len(), want);
                }
            }
        });
    }

    /// The image a colour glyph is drawn as: what PDF/A-1b checks for
    /// transparency.
    #[test]
    fn image_of_colour_glyphs() {
        with_fonts(|fonts| {
            let (colr, cbdt) = (fonts.get(0), fonts.get(1));
            let glyph = |f: &LoadedFont<'_>, c: char| f.glyph(c).unwrap();
            let (grin, heart) = (glyph(colr, '😀'), glyph(colr, '\u{2764}'));
            let space = glyph(colr, ' ');
            let used = BTreeMap::from([(0, BTreeSet::from([grin, heart, space]))]);
            let mut used = black(&used);
            used.insert((1, glyph(cbdt, '😀')), BTreeSet::from([BLACK_TEXT]));
            // In two text colours: the image of the first.
            used.insert((0, heart), BTreeSet::from([[200, 0, 0], BLACK_TEXT]));
            let mut images = no_images();
            let colors = ColorGlyphs::load(fonts, &used, &mut images, false);
            let transparency = |font, gid| {
                let image = colors.image(font, gid, &images)?;
                let one_b = crate::pdf::Profile::new(Some(crate::PdfALevel::A1b));
                crate::pdf::image_transparency(image, one_b)
            };
            // Binary alpha: a stencil mask.
            assert_eq!(transparency(1, glyph(cbdt, '😀')), None);
            assert!(transparency(0, grin).is_some());
            // Solid layers only.
            assert!(colors.image(0, heart, &images).is_some());
            assert_eq!(transparency(0, heart), None);
            // No colour.
            assert!(colors.image(0, space, &images).is_none());
        });
    }
}
