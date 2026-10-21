//! Image `XObjects`: JPEG passthrough (`DCTDecode`) or Flate samples with an
//! optional soft mask for alpha. SVG becomes a Form `XObject` (`svg.rs`).
//! PDF/A output has an sRGB output intent only, so CMYK JPEGs are
//! converted to RGB samples there.

use std::sync::Arc;

use pdf_writer::{Filter, Ref};

use super::{Profile, Writer};
use crate::images::{ContentKey, DecodedImage, ImageBody, JpegColor, RawColor};
use crate::lru::SharedLru;

/// Bytes of RGB samples kept across renders: converting a CMYK JPEG is
/// most of the cost of writing it to PDF/A.
const RGB_BUDGET: usize = 32 << 20;

/// The compressed RGB samples of CMYK JPEGs written to PDF/A, by the
/// JPEG's bytes.
pub(crate) static CMYK_AS_RGB: SharedLru<ContentKey<Arc<[u8]>>, Arc<[u8]>> =
    SharedLru::new(RGB_BUDGET);

/// What needs transparency in an image whose alpha is neither clear nor
/// opaque in places.
pub(crate) const PARTIAL_ALPHA: &str = "partial alpha; the `flattenAlpha` option draws it on white";

/// What in `img` needs transparency, which PDF/A-1 forbids, written with
/// `profile`: partial alpha (written as a soft mask) unless the profile
/// flattens it, or what [`super::svg::transparency`] finds in an SVG.
/// Binary alpha is written as a stencil mask, without transparency.
pub(crate) fn transparency(img: &DecodedImage, profile: Profile) -> Option<&'static str> {
    match &img.body {
        ImageBody::Raw {
            alpha: Some(alpha), ..
        } => (!alpha.binary && !profile.flattens_alpha()).then_some(PARTIAL_ALPHA),
        ImageBody::Svg {
            probed: Some(probed),
            tree,
            ..
        } => super::svg::transparency(tree, Some(&probed.probes), profile),
        // Its text is drawn with the paint of its probes: scan a conversion
        // with a probe for each text and paint style, whatever the fonts.
        ImageBody::Svg {
            text: Some(source),
            tree,
            ..
        } => crate::images::svg::probe_tree(source, tree.size())
            .ok()
            .and_then(|(tree, probed)| {
                super::svg::transparency(&tree, Some(&probed.probes), profile)
            }),
        ImageBody::Svg { tree, .. } => super::svg::transparency(tree, None, profile),
        ImageBody::Raw { .. } | ImageBody::Jpeg { .. } => None,
    }
}

/// Writes SVG `img` with its parts tagged, once for each `/StructParents`
/// key of `keys` ([`super::svg::write_tagged_svg`]); `None` if it is not
/// an SVG with parts.
pub(crate) fn write_tagged_svg(
    w: &mut Writer,
    img: &DecodedImage,
    profile: Profile,
    fonts: Option<super::svg::TextResources<'_, '_>>,
    (outer, keys): (&'static [u8], &[i32]),
) -> Result<Option<(Vec<Ref>, super::svg::FormTags)>, crate::InkoxideError> {
    let ImageBody::Svg {
        tree,
        probed,
        parts: Some(parts),
        ..
    } = &img.body
    else {
        return Ok(None);
    };
    let size = probed.as_ref().map_or(tree.size(), |p| p.size);
    let text = probed.as_deref().zip(fonts);
    super::svg::write_tagged_svg(w, (tree, size), profile, text, (parts, outer, keys)).map(Some)
}

/// Writes `img`; `fonts` draw the text of a render's copy of an SVG.
pub(crate) fn write_image(
    w: &mut Writer,
    img: &DecodedImage,
    profile: Profile,
    fonts: Option<super::svg::TextResources<'_, '_>>,
) -> Result<Ref, crate::InkoxideError> {
    if let ImageBody::Svg { tree, probed, .. } = &img.body {
        // A render's copy has the size of the SVG without its text.
        let size = probed.as_ref().map_or(tree.size(), |p| p.size);
        let text = probed.as_deref().zip(fonts);
        return super::svg::write_svg(w, (tree, size), profile, text);
    }
    let Writer {
        pdf, alloc, out, ..
    } = w;
    let id = alloc.next();
    let (w, h) = (img.width as i32, img.height as i32);
    match &img.body {
        ImageBody::Jpeg {
            bytes,
            color: JpegColor::Cmyk { .. },
        } if profile.is_pdf_a() => {
            // Converted once, for this render and later ones.
            let key = ContentKey::new(bytes.clone());
            let samples = match CMYK_AS_RGB.get(&key) {
                Some(samples) => samples,
                None => {
                    let samples: Arc<[u8]> =
                        super::deflate(&img.cmyk_to_rgb()?, crate::flate::LEVEL)?.into();
                    CMYK_AS_RGB.insert(key, samples.clone(), samples.len());
                    samples
                }
            };
            out.image_xobject(pdf, id, &samples, |x| {
                x.width(w).height(h).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                x.color_space().device_rgb();
            });
        }
        ImageBody::Jpeg { bytes, color } => out.image_xobject(pdf, id, bytes, |x| {
            x.width(w).height(h).bits_per_component(8);
            x.filter(Filter::DctDecode);
            match color {
                JpegColor::Gray => {
                    x.color_space().device_gray();
                }
                JpegColor::Rgb => {
                    x.color_space().device_rgb();
                }
                JpegColor::Cmyk { inverted } => {
                    x.color_space().device_cmyk();
                    if *inverted {
                        x.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
                    }
                }
            }
        }),
        ImageBody::Raw {
            color,
            samples,
            alpha,
        } => {
            // Where transparency is forbidden, binary alpha is a stencil
            // mask, and partial alpha is drawn on white when asked.
            let flattened;
            let mut samples = &samples[..];
            let mask = match alpha {
                None => None,
                Some(a) if profile.transparency() => {
                    let m = alloc.next();
                    out.image_xobject(pdf, m, &a.plane, |x| {
                        x.width(w).height(h).bits_per_component(8);
                        x.filter(Filter::FlateDecode);
                        x.color_space().device_gray();
                    });
                    Some(Mask::Soft(m))
                }
                Some(a) if a.binary => {
                    let bits = super::deflate(&img.stencil_mask()?, crate::flate::LEVEL)?;
                    let m = alloc.next();
                    out.image_xobject(pdf, m, &bits, |x| {
                        x.width(w).height(h).bits_per_component(1);
                        x.filter(Filter::FlateDecode);
                        x.image_mask(true);
                    });
                    Some(Mask::Stencil(m))
                }
                Some(_) => {
                    if !profile.flattens_alpha() {
                        profile.allow_transparency(PARTIAL_ALPHA)?;
                    }
                    flattened = super::deflate(&img.flattened_samples()?, crate::flate::LEVEL)?;
                    samples = &flattened;
                    None
                }
            };
            out.image_xobject(pdf, id, samples, |x| {
                x.width(w).height(h).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                match color {
                    RawColor::Gray => x.color_space().device_gray(),
                    RawColor::Rgb => x.color_space().device_rgb(),
                }
                match mask {
                    Some(Mask::Soft(m)) => x.s_mask(m),
                    Some(Mask::Stencil(m)) => x.stencil_mask(m),
                    None => x,
                };
            });
        }
        ImageBody::Svg { .. } => unreachable!("handled above"),
    }
    Ok(id)
}

/// The mask an image's alpha is written as.
enum Mask {
    /// A soft mask, which is transparency.
    Soft(Ref),
    /// An explicit stencil mask, of binary alpha (ISO 32000-1, 8.9.6.3).
    Stencil(Ref),
}
