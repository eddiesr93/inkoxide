//! Image XObjects: JPEG passthrough (`DCTDecode`) or Flate samples with an
//! optional soft mask for alpha. SVG becomes a Form XObject (`svg.rs`).
//! PDF/A output has an sRGB output intent only, so CMYK JPEGs are
//! converted to RGB samples there (ADR-039).

use pdf_writer::{Filter, Pdf, Ref};

use crate::images::{DecodedImage, ImageBody, JpegColor, RawColor};

pub(crate) fn write_image(
    pdf: &mut Pdf,
    alloc: &mut dyn FnMut() -> Ref,
    img: &DecodedImage,
    pdf_a: bool,
) -> Result<Ref, crate::InkoxideError> {
    if let ImageBody::Svg(tree) = &img.body {
        return super::svg::write_svg(pdf, alloc, tree, pdf_a);
    }
    let id = alloc();
    let (w, h) = (img.width as i32, img.height as i32);
    match &img.body {
        ImageBody::Jpeg {
            bytes,
            color: JpegColor::Cmyk { .. },
        } if pdf_a => {
            let rgb = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
                .map_err(|e| crate::InkoxideError::Image(format!("CMYK JPEG: {e}")))?
                .into_rgb8();
            if (rgb.width(), rgb.height()) != (img.width, img.height) {
                return Err(crate::InkoxideError::Internal(
                    "CMYK JPEG decoded to an unexpected size".into(),
                ));
            }
            let samples = super::deflate(rgb.as_raw(), 6)?;
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
                let m = alloc();
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
            };
            if let Some(m) = mask {
                x.s_mask(m);
            }
        }
        ImageBody::Svg(_) => unreachable!("handled above"),
    }
    Ok(id)
}
