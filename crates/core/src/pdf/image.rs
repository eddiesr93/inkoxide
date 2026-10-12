//! Image `XObjects`: JPEG passthrough (`DCTDecode`) or Flate samples with an
//! optional soft mask for alpha. SVG becomes a Form `XObject` (`svg.rs`).
//! PDF/A output has an sRGB output intent only, so CMYK JPEGs are
//! converted to RGB samples there.

use pdf_writer::{Filter, Pdf, Ref};

use super::RefAlloc;
use crate::images::{DecodedImage, ImageBody, JpegColor, RawColor};

pub(crate) fn write_image(
    pdf: &mut Pdf,
    alloc: &mut RefAlloc,
    img: &DecodedImage,
    pdf_a: bool,
) -> Result<Ref, crate::InkoxideError> {
    if let ImageBody::Svg { tree, .. } = &img.body {
        return super::svg::write_svg(pdf, alloc, tree, pdf_a);
    }
    let id = alloc.next();
    let (w, h) = (img.width as i32, img.height as i32);
    match &img.body {
        ImageBody::Jpeg {
            color: JpegColor::Cmyk { .. },
            ..
        } if pdf_a => {
            let samples = super::deflate(&img.cmyk_to_rgb()?, crate::flate::LEVEL)?;
            let mut x = pdf.image_xobject(id, &samples);
            x.width(w).height(h).bits_per_component(8);
            x.filter(Filter::FlateDecode);
            x.color_space().device_rgb();
        }
        ImageBody::Jpeg { bytes, color } => {
            let mut x = pdf.image_xobject(id, bytes);
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
        }
        ImageBody::Raw {
            color,
            samples,
            alpha,
        } => {
            let mask = alpha.as_ref().map(|a| {
                let m = alloc.next();
                let mut x = pdf.image_xobject(m, a);
                x.width(w).height(h).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                x.color_space().device_gray();
                m
            });
            let mut x = pdf.image_xobject(id, samples);
            x.width(w).height(h).bits_per_component(8);
            x.filter(Filter::FlateDecode);
            match color {
                RawColor::Gray => x.color_space().device_gray(),
                RawColor::Rgb => x.color_space().device_rgb(),
            }
            if let Some(m) = mask {
                x.s_mask(m);
            }
        }
        ImageBody::Svg { .. } => unreachable!("handled above"),
    }
    Ok(id)
}
