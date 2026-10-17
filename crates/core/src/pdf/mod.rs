//! Display list → PDF bytes, through `pdf-writer`.

mod attach;
mod content;
pub(crate) mod crypt;
mod document;
mod font;
mod forms;
mod image;
mod nav;
mod notes;
mod out;
mod pdfa;
mod svg;
mod tag;

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::{Content, Pdf, Ref};
use rayon::prelude::*;

use crate::date::Timestamp;
use crate::fonts::{FontKey, FontSet};
use crate::geom::Rect;
use crate::images::{ImageKey, ImageSet};
use crate::paint::PageContent;
use crate::pipeline::Marks;
use crate::schema::Meta;
use crate::text::UnicodeMap;
use crate::InkoxideError;
pub(crate) use font::SUBSETS;

use content::{
    alpha_key, bounds, clip, fill, font_name, gs_name, image_name, invisible_glyph, key_alpha,
    path, set_fill, show_kerned, show_text,
};
pub(crate) use image::transparency as image_transparency;
pub(crate) use nav::written_uri_len;
use nav::{annot_rect, write_link, write_outline};
use out::Out;
pub(crate) use out::{most_written_text_len, written_text_len};
pub(crate) use pdfa::{Profile, MIN_PAGE_EDGE as PDF_A_MIN_PAGE_EDGE};

/// A painted page reduced to what the writer needs: its compressed
/// content stream and the resources it names. Pages are encoded as soon as
/// they are painted (in parallel), so the display lists never pile up.
pub(crate) struct EncodedPage {
    pub width: f32,
    pub height: f32,
    /// Content stream, Flate-compressed when `compressed`.
    pub stream: Vec<u8>,
    pub compressed: bool,
    /// Glyphs used per font.
    pub glyphs: BTreeMap<FontKey, BTreeSet<u16>>,
    /// Opacity keys (see `alpha_key`).
    pub opacities: BTreeSet<u16>,
    pub images: BTreeSet<ImageKey>,
    /// Link areas (page coordinates, top-left origin) and their targets.
    pub links: Vec<(Rect, Box<str>)>,
    /// Form field widgets.
    pub fields: Vec<crate::forms::FieldOp>,
    /// Icon annotations (text notes), at the corner the transform in
    /// effect puts them.
    pub icons: Vec<crate::icons::IconOp>,
    /// In tagged output, the elements of its content and annotations.
    pub tags: Option<content::PageTags>,
}

pub(crate) struct DocumentOut<'a> {
    pub pages: &'a [EncodedPage],
    pub meta: &'a Meta,
    pub creation_date: Option<Timestamp>,
    pub fonts: &'a FontSet<'a>,
    pub unicode: &'a UnicodeMap,
    pub images: &'a ImageSet,
    pub colors: &'a crate::text::ColorGlyphs,
    pub marks: &'a Marks<'a>,
    /// The files the document carries.
    pub files: &'a [crate::attachments::PreparedFile],
    /// The structure of tagged output.
    pub plan: Option<&'a crate::tag::Plan>,
    /// Writing PDF/UA-1.
    pub ua: bool,
}

pub(crate) struct PdfWriterSink {
    /// Flate level for content streams; `None` writes them uncompressed.
    pub compression: Option<u32>,
    /// Embed only the glyphs used.
    pub subset_fonts: bool,
    /// Plain PDF or PDF/A.
    pub profile: Profile,
    /// Encrypt the file with these passwords and permissions.
    pub encryption: Option<crypt::Params>,
    /// Prepare font programs on the current rayon pool; otherwise on the
    /// calling thread.
    pub parallel: bool,
}

/// Largest magnitude of a number the writer emits (see [`number`]).
///
/// PDF readers parse reals up to about ±3.4e38 but integers only within 32
/// bits (PDF 1.7, Annex C), and `pdf-writer` writes a value of 1e12 or more
/// as digits without a decimal point: an integer far past 2^31 − 1, which
/// readers reject or read as null (qpdf: "overflow/underflow converting
/// … to 64-bit integer"). Below 1e12 it writes a real, or an integer when
/// the value is one within 32 bits. 1e11 stays clear of that edge and
/// above anything a document draws: lengths are at most 1e6 pt, and the
/// largest scale an SVG within the image limits needs is 1e10 (its
/// largest size over its smallest `viewBox`). 1e11 pt is some 35,000 km,
/// so clamping there only moves what had already overflowed any page.
pub(crate) const MAX_NUMBER: f32 = 1e11;

/// `v` as a number every PDF reader parses: 0 when it is infinite or NaN,
/// and within ±[`MAX_NUMBER`]. The backstop for every number this module
/// writes; numbers that cannot exceed it by construction (colours,
/// opacities, font metrics in font units over at least 16 units per em,
/// gradient stops and periods) are written as they are.
///
/// PDF has no syntax for infinity or NaN; `pdf-writer` would write `inf` or
/// `NaN`, which makes the content stream invalid, and viewers reject the
/// page. Validation bounds every number in a document, but bounded values
/// can still compound past `f32` (a percentage of a percentage, a few
/// dozen levels deep, or one node's scales multiplied), and SVG images
/// bring numbers of their own (`scale(1e30)`). So this is not a
/// `debug_assert`: a valid document must not panic a debug build (the
/// fuzz targets and the debug Node binding run with assertions on). Zero
/// or the bound keeps the file valid; only the shape whose geometry
/// overflowed is misplaced. Other values pass unchanged, tiny ones too,
/// so output is byte-identical, and the check is a few compares.
#[inline]
pub(crate) fn number(v: f32) -> f32 {
    /// 2^31: `pdf-writer` writes ±2^31 as 32-bit integers (2^31 − 1 for
    /// 2^31), and in a debug build panics registering −2^31. The `f32`
    /// next to it, toward zero, is an integer within range.
    const I32_EDGE: f32 = 2_147_483_648.0;
    if !v.is_finite() {
        0.0
    } else if v.abs() == I32_EDGE {
        v.signum() * 2_147_483_520.0
    } else {
        v.clamp(-MAX_NUMBER, MAX_NUMBER)
    }
}

/// Rounds to 1/1000 pt so output has at most three decimals; bounded and
/// never infinite or NaN (see [`number`]).
#[inline]
pub(crate) fn round_pt(v: f32) -> f32 {
    round_to(v, 1000.0)
}

/// Rounds to `1 / per`, through [`number`]. Past about 3e35 points the
/// scaling overflows, but `f32` has no fraction left to round there.
#[inline]
pub(super) fn round_to(v: f32, per: f32) -> f32 {
    let r = (v * per).round() / per;
    number(if r.is_finite() { r } else { v })
}

/// The bytes of content stream `c`, whose numbers went through
/// [`number`] (checked in debug builds), within the limits of `profile`.
pub(crate) fn content_bytes(c: Content, profile: Profile) -> Result<Vec<u8>, InkoxideError> {
    let buf = c.finish();
    debug_assert!(
        buf.limits().real() <= MAX_NUMBER,
        "a number past MAX_NUMBER: {}",
        buf.limits().real()
    );
    profile.check_limits(buf.limits())?;
    let data = buf.into_vec();
    profile.check_nesting(&data)?;
    Ok(data)
}

/// Hands out object numbers in order, from 1.
pub(super) struct RefAlloc(i32);

impl RefAlloc {
    pub(super) fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

/// Where every object goes on its way into the file: the file, the
/// object numbers handed out so far, and the strings and streams,
/// encrypted when the file is.
pub(super) struct Writer {
    pub pdf: Pdf,
    pub alloc: RefAlloc,
    pub out: Out,
}

impl Writer {
    fn new(out: Out) -> Self {
        Self {
            pdf: Pdf::new(),
            alloc: RefAlloc(0),
            out,
        }
    }
}

impl PdfWriterSink {
    /// Encodes one page, tagged with `plan`; called from worker threads.
    pub fn encode_page(
        &self,
        page: &PageContent,
        fonts: &FontSet<'_>,
        plan: Option<&crate::tag::Plan>,
    ) -> Result<EncodedPage, InkoxideError> {
        content::encode_page(page, fonts, self.profile, self.compression, plan)
    }

    pub fn write(&mut self, doc: &DocumentOut<'_>) -> Result<Vec<u8>, InkoxideError> {
        let fonts = self.font_programs(doc)?;
        let Some(params) = &self.encryption else {
            return Ok(self.emit(doc, &fonts, Out::plain())?.pdf.finish());
        };
        // The keys are derived from the whole file as it is written
        // unencrypted, so documents that differ anywhere get different
        // keys.
        let plain = self.emit(doc, &fonts, Out::plain())?.pdf.finish();
        let (crypt, dict) = crypt::Crypt::new(params, &crypt::digest(&plain));
        drop(plain);
        let Writer {
            mut pdf, mut alloc, ..
        } = self.emit(doc, &fonts, Out::encrypted(crypt))?;
        // Last, so every other object keeps its number.
        let r = alloc.next();
        dict.write(&mut pdf, r);
        pdf.set_file_id((dict.id.to_vec(), dict.id.to_vec()));
        crypt::splice_encrypt(pdf.finish(), r)
            .ok_or_else(|| InkoxideError::Pdf("the file has no trailer to encrypt".into()))
    }

    /// Every font that is actually painted, with the glyphs used across
    /// the whole document, subset and compressed.
    fn font_programs(&self, doc: &DocumentOut<'_>) -> Result<FontPrograms, InkoxideError> {
        let mut used: BTreeMap<FontKey, BTreeSet<u16>> = BTreeMap::new();
        let mut editable = BTreeSet::new();
        let mut field_text = UnicodeMap::default();
        let mut buf = [0; 4];
        for page in doc.pages {
            for (key, glyphs) in &page.glyphs {
                used.entry(*key).or_default().extend(glyphs);
            }
            for f in &page.fields {
                let Some(font) = f.font else { continue };
                if f.editable {
                    editable.insert(font);
                }
                for &(glyph, c) in &f.glyph_chars {
                    field_text.record(font, glyph, c.encode_utf8(&mut buf));
                }
            }
        }
        for key in editable {
            let font = doc.fonts.get(key);
            let glyphs = used.entry(key).or_default();
            for (glyph, c) in crate::forms::edit_glyphs(font) {
                glyphs.insert(glyph);
                field_text.record(key, glyph, c.encode_utf8(&mut buf));
            }
        }
        // The glyphs of SVG text laid out for the images painted: every
        // text, drawn or not, so a superset of what the forms show.
        for (glyphs, _) in svg_text_uses(doc) {
            for (font, glyph) in glyphs {
                used.entry(font).or_default().insert(glyph);
            }
        }
        // Subsetting and compression are independent per font.
        let program = |(key, glyphs): (&FontKey, &BTreeSet<u16>)| {
            font::program(doc.fonts.get(*key), glyphs, self.subset_fonts)
        };
        let programs: Vec<font::Program> = if self.parallel {
            used.par_iter().map(program).collect::<Result<_, _>>()?
        } else {
            used.iter().map(program).collect::<Result<_, _>>()?
        };
        Ok(FontPrograms {
            used,
            programs,
            field_text,
        })
    }
}

/// What the text of each SVG painted in `doc` draws besides itself
/// ([`svg::text_uses`]).
fn svg_text_uses<'a>(
    doc: &'a DocumentOut<'a>,
) -> impl Iterator<Item = (Vec<(FontKey, u16)>, Vec<ImageKey>)> + 'a {
    let painted: BTreeSet<ImageKey> = doc.pages.iter().flat_map(|p| &p.images).copied().collect();
    painted.into_iter().filter_map(move |key| {
        let crate::images::ImageBody::Svg {
            probed: Some(probed),
            ..
        } = &doc.images.get(key).body
        else {
            return None;
        };
        Some(svg::text_uses(probed, text_ink(doc)))
    })
}

/// What the text of the SVGs in `doc` is drawn with.
fn text_ink<'a>(doc: &DocumentOut<'a>) -> svg::TextInk<'a, 'a> {
    svg::TextInk {
        fonts: doc.fonts,
        colors: doc.colors,
        images: doc.images,
    }
}

/// The font programs a document embeds, and the glyphs each one holds.
struct FontPrograms {
    used: BTreeMap<FontKey, BTreeSet<u16>>,
    programs: Vec<font::Program>,
    /// The text of the glyphs form fields show or embed for editing, which
    /// layout never shaped.
    field_text: UnicodeMap,
}

fn deflate(data: &[u8], level: u32) -> Result<Vec<u8>, InkoxideError> {
    crate::flate::deflate(data, level)
        .map_err(|e| InkoxideError::Pdf(format!("compression failed: {e}")))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod pdf_a_1_tests;
