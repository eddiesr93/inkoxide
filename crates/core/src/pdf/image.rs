//! Image `XObjects`: JPEG passthrough (`DCTDecode`) or Flate samples with an
//! optional soft mask for alpha. SVG becomes a Form `XObject` (`svg.rs`).
//! PDF/A output has an sRGB output intent only, so CMYK JPEGs are
//! converted to RGB samples there.

use pdf_writer::{Filter, Pdf, Ref};

use super::{Out, Profile, RefAlloc};
use crate::images::{DecodedImage, ImageBody, JpegColor, RawColor};

/// What in `img` needs transparency, which PDF/A-1 forbids: an alpha
/// channel (written as a soft mask), or what [`super::svg::transparency`]
/// finds in an SVG.
pub(crate) fn transparency(img: &DecodedImage) -> Option<&'static str> {
    match &img.body {
        ImageBody::Raw { alpha: Some(_), .. } => Some("an alpha channel"),
        ImageBody::Svg {
            probed: Some(probed),
            tree,
            ..
        } => super::svg::transparency(tree, Some(&probed.probes)),
        // Its text is drawn with the paint of its probes: scan a conversion
        // with a probe for each text and paint style, whatever the fonts.
        ImageBody::Svg {
            text: Some(source),
            tree,
            ..
        } => crate::images::svg::probe_tree(source, tree.size())
            .ok()
            .and_then(|(tree, probed)| super::svg::transparency(&tree, Some(&probed.probes))),
        ImageBody::Svg { tree, .. } => super::svg::transparency(tree, None),
        ImageBody::Raw { .. } | ImageBody::Jpeg { .. } => None,
    }
}

/// Writes `img`; `fonts` draw the text of a render's copy of an SVG.
pub(crate) fn write_image(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    out: &mut Out,
    img: &DecodedImage,
    profile: Profile,
    fonts: Option<super::svg::TextResources<'_, '_>>,
) -> Result<Ref, crate::InkoxideError> {
    if let ImageBody::Svg { tree, probed, .. } = &img.body {
        // A render's copy has the size of the SVG without its text.
        let size = probed.as_ref().map_or(tree.size(), |p| p.size);
        let text = probed.as_deref().zip(fonts);
        return super::svg::write_svg(pdf, alloc, out, (tree, size), profile, text);
    }
    let id = alloc.next();
    let (w, h) = (img.width as i32, img.height as i32);
    match &img.body {
        ImageBody::Jpeg {
            color: JpegColor::Cmyk { .. },
            ..
        } if profile.is_pdf_a() => {
            let samples = super::deflate(&img.cmyk_to_rgb()?, crate::flate::LEVEL)?;
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
            if alpha.is_some() {
                profile.allow_transparency("an image with an alpha channel")?;
            }
            let mask = alpha.as_ref().map(|a| {
                let m = alloc.next();
                out.image_xobject(pdf, m, a, |x| {
                    x.width(w).height(h).bits_per_component(8);
                    x.filter(Filter::FlateDecode);
                    x.color_space().device_gray();
                });
                m
            });
            out.image_xobject(pdf, id, samples, |x| {
                x.width(w).height(h).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                match color {
                    RawColor::Gray => x.color_space().device_gray(),
                    RawColor::Rgb => x.color_space().device_rgb(),
                }
                if let Some(m) = mask {
                    x.s_mask(m);
                }
            });
        }
        ImageBody::Svg { .. } => unreachable!("handled above"),
    }
    Ok(id)
}
