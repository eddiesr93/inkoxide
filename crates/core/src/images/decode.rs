//! Image decoding behind the [`ImageDecoder`] firewall.

use std::sync::Arc;

use super::probe::{probe, ImageFormat};

/// Colour space of a passthrough JPEG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JpegColor {
    Gray,
    Rgb,
    /// CMYK; `inverted` for Adobe-written files (needs a `Decode` array).
    Cmyk {
        inverted: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawColor {
    Gray,
    Rgb,
}

#[derive(Debug)]
pub(crate) enum ImageBody {
    /// The original JPEG bytes, embedded with `DCTDecode`.
    Jpeg { bytes: Arc<[u8]>, color: JpegColor },
    /// 8-bit samples, zlib-compressed (`FlateDecode`), with an optional
    /// zlib-compressed 8-bit alpha plane.
    Raw {
        color: RawColor,
        samples: Vec<u8>,
        alpha: Option<Vec<u8>>,
    },
    /// A parsed SVG, written as a Form `XObject` with vector content, and
    /// its root `viewBox`.
    Svg {
        tree: Arc<usvg::Tree>,
        view_box: Option<super::svg::ViewBox>,
    },
}

#[derive(Debug)]
pub(crate) struct DecodedImage {
    /// Stored pixel size.
    pub width: u32,
    pub height: u32,
    /// EXIF orientation (1–8).
    pub orientation: u8,
    pub body: ImageBody,
}

impl DecodedImage {
    /// Size as displayed in CSS pixels (orientations 5–8 swap the axes;
    /// SVG keeps its fractional size).
    pub fn display_size(&self) -> (f32, f32) {
        if let ImageBody::Svg { tree, .. } = &self.body {
            let s = tree.size();
            return (s.width(), s.height());
        }
        if self.orientation >= 5 {
            (self.height as f32, self.width as f32)
        } else {
            (self.width as f32, self.height as f32)
        }
    }

    /// The 8-bit RGB samples of a CMYK JPEG, for output that cannot hold
    /// CMYK (PDF/A has an sRGB output intent only). Decoded on every call.
    pub fn cmyk_to_rgb(&self) -> Result<Vec<u8>, crate::InkoxideError> {
        let ImageBody::Jpeg {
            bytes,
            color: JpegColor::Cmyk { .. },
        } = &self.body
        else {
            return Err(crate::InkoxideError::Internal("not a CMYK JPEG".into()));
        };
        let rgb = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
            .map_err(|e| crate::InkoxideError::Image(format!("CMYK JPEG: {e}")))?
            .into_rgb8();
        if (rgb.width(), rgb.height()) != (self.width, self.height) {
            return Err(crate::InkoxideError::Internal(
                "CMYK JPEG decoded to an unexpected size".into(),
            ));
        }
        Ok(rgb.into_raw())
    }
}

pub(crate) trait ImageDecoder: Sync {
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage, String>;
}

/// JPEG passthrough plus PNG, GIF and WebP decoding with the `image` crate.
pub(crate) struct ImageCrateDecoder;

impl ImageDecoder for ImageCrateDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage, String> {
        if super::svg::is_svg(bytes) {
            let (tree, view_box) = super::svg::parse(bytes)?;
            let s = tree.size();
            return Ok(DecodedImage {
                width: s.width().ceil() as u32,
                height: s.height().ceil() as u32,
                orientation: 1,
                body: ImageBody::Svg {
                    tree: Arc::new(tree),
                    view_box,
                },
            });
        }
        let info = probe(bytes)?;
        match info.format {
            ImageFormat::Jpeg => Ok(DecodedImage {
                width: info.width,
                height: info.height,
                orientation: info.orientation,
                body: ImageBody::Jpeg {
                    bytes: bytes.into(),
                    color: match info.components {
                        1 => JpegColor::Gray,
                        3 => JpegColor::Rgb,
                        _ => JpegColor::Cmyk {
                            inverted: info.adobe,
                        },
                    },
                },
            }),
            ImageFormat::Png => decode_raster(bytes, image::ImageFormat::Png),
            // The first frame of an animation.
            ImageFormat::Gif => decode_raster(bytes, image::ImageFormat::Gif),
            ImageFormat::WebP => decode_raster(bytes, image::ImageFormat::WebP),
            ImageFormat::Svg => Err("SVG was not recognised".into()),
        }
    }
}

fn zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    crate::flate::deflate(data, crate::flate::LEVEL).map_err(|e| format!("compression failed: {e}"))
}

/// PNG, GIF and WebP: decoded to 8-bit samples.
fn decode_raster(bytes: &[u8], format: image::ImageFormat) -> Result<DecodedImage, String> {
    use image::ColorType;
    let img = image::load_from_memory_with_format(bytes, format).map_err(|e| {
        let name = format.extensions_str().first().copied().unwrap_or("image");
        format!("{} could not be decoded: {e}", name.to_uppercase())
    })?;
    let (width, height) = (img.width(), img.height());
    let color = img.color();
    let gray = matches!(
        color,
        ColorType::L8 | ColorType::La8 | ColorType::L16 | ColorType::La16
    );
    let (samples, alpha_plane, raw_color) = if color.has_alpha() {
        if gray {
            let la = img.into_luma_alpha8();
            let px = la.as_raw();
            let g: Vec<u8> = px.chunks_exact(2).map(|p| p[0]).collect();
            let a: Vec<u8> = px.chunks_exact(2).map(|p| p[1]).collect();
            (g, Some(a), RawColor::Gray)
        } else {
            let rgba = img.into_rgba8();
            let px = rgba.as_raw();
            let mut rgb = Vec::with_capacity(px.len() / 4 * 3);
            let mut a = Vec::with_capacity(px.len() / 4);
            for p in px.chunks_exact(4) {
                rgb.extend_from_slice(&p[..3]);
                a.push(p[3]);
            }
            (rgb, Some(a), RawColor::Rgb)
        }
    } else if gray {
        (img.into_luma8().into_raw(), None, RawColor::Gray)
    } else {
        (img.into_rgb8().into_raw(), None, RawColor::Rgb)
    };
    // A fully opaque alpha plane is dropped (no SMask needed).
    let alpha = match alpha_plane {
        Some(a) if a.iter().any(|&v| v != 255) => Some(zlib(&a)?),
        _ => None,
    };
    Ok(DecodedImage {
        width,
        height,
        orientation: 1,
        body: ImageBody::Raw {
            color: raw_color,
            samples: zlib(&samples)?,
            alpha,
        },
    })
}
