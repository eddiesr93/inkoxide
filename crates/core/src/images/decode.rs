//! Image decoding behind the [`ImageDecoder`] firewall (§5.1).

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
    /// Pixel size as displayed (orientations 5–8 swap the axes).
    pub fn display_size(&self) -> (u32, u32) {
        if self.orientation >= 5 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

pub(crate) trait ImageDecoder: Sync {
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage, String>;
}

/// JPEG passthrough plus PNG decoding with the `image` crate.
pub(crate) struct ImageCrateDecoder;

/// Flate level for image samples: decoded once and cached, so favour size.
const IMAGE_FLATE_LEVEL: u32 = 6;

impl ImageDecoder for ImageCrateDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<DecodedImage, String> {
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
            ImageFormat::Png => decode_png(bytes),
        }
    }
}

fn zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    crate::pdf::deflate(data, IMAGE_FLATE_LEVEL).map_err(|e| e.to_string())
}

fn decode_png(bytes: &[u8]) -> Result<DecodedImage, String> {
    use image::ColorType;
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| format!("PNG could not be decoded: {e}"))?;
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
