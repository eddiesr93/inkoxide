//! Header-only inspection of JPEG, PNG, GIF and WebP files (SVG is parsed): format, pixel size,
//! components and EXIF orientation. Used by validation (cheap, no decode)
//! and by the JPEG passthrough path.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageFormat {
    Jpeg,
    Png,
    Gif,
    WebP,
    Svg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// JPEG colour components (1, 3 or 4); 0 for PNG.
    pub components: u8,
    /// JPEG with an Adobe APP14 marker (CMYK data is then stored inverted).
    pub adobe: bool,
    /// EXIF orientation 1–8 (1 when absent).
    pub orientation: u8,
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Why [`probe`] rejected an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProbeError {
    /// Not a supported image, or its header is malformed.
    Invalid(String),
    /// More pixels than [`super::MAX_PIXELS`].
    TooLarge(String),
}

impl From<String> for ProbeError {
    fn from(e: String) -> Self {
        Self::Invalid(e)
    }
}

impl From<ProbeError> for String {
    fn from(e: ProbeError) -> Self {
        match e {
            ProbeError::Invalid(m) | ProbeError::TooLarge(m) => m,
        }
    }
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) | Self::TooLarge(m) => f.write_str(m),
        }
    }
}

pub(crate) fn probe(bytes: &[u8]) -> Result<ImageInfo, ProbeError> {
    let info = if bytes.starts_with(PNG_SIGNATURE) {
        probe_png(bytes)?
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        probe_jpeg(bytes)?
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        probe_gif(bytes)?
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        probe_webp(bytes)?
    } else if super::svg::is_svg(bytes) {
        // No header to read: parsing is the only real check.
        let size = super::svg::parse(bytes)?.0.size();
        return Ok(ImageInfo {
            format: ImageFormat::Svg,
            width: size.width().ceil() as u32,
            height: size.height().ceil() as u32,
            components: 0,
            adobe: false,
            orientation: 1,
        });
    } else {
        return Err(ProbeError::Invalid(
            "unsupported image format (expected JPEG, PNG, GIF, WebP or SVG)".into(),
        ));
    };
    if info.width == 0 || info.height == 0 {
        return Err(ProbeError::Invalid("image has zero width or height".into()));
    }
    let pixels = u64::from(info.width) * u64::from(info.height);
    if pixels > super::MAX_PIXELS {
        return Err(ProbeError::TooLarge(format!(
            "image is {}×{} pixels; the limit is {} megapixels",
            info.width,
            info.height,
            super::MAX_PIXELS / 1_000_000
        )));
    }
    Ok(info)
}

fn be16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn probe_png(b: &[u8]) -> Result<ImageInfo, String> {
    // Signature, then IHDR: length(4) "IHDR" width(4) height(4).
    if b.get(12..16) != Some(b"IHDR") {
        return Err("PNG is missing its IHDR chunk".into());
    }
    let (w, h) = be32(b, 16)
        .zip(be32(b, 20))
        .ok_or("PNG header is truncated")?;
    Ok(ImageInfo {
        format: ImageFormat::Png,
        width: w,
        height: h,
        components: 0,
        adobe: false,
        orientation: 1,
    })
}

fn raster(format: ImageFormat, width: u32, height: u32) -> ImageInfo {
    ImageInfo {
        format,
        width,
        height,
        components: 0,
        adobe: false,
        orientation: 1,
    }
}

fn le16(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?])))
}

fn le24(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(i)?,
        *b.get(i + 1)?,
        *b.get(i + 2)?,
        0,
    ]))
}

/// Logical screen size of a GIF (the first frame is drawn on it).
fn probe_gif(b: &[u8]) -> Result<ImageInfo, String> {
    let (w, h) = le16(b, 6)
        .zip(le16(b, 8))
        .ok_or("GIF header is truncated")?;
    Ok(raster(ImageFormat::Gif, w, h))
}

/// Canvas size from the first chunk: `VP8X` (extended), `VP8L`
/// (lossless) or `VP8 ` (lossy).
fn probe_webp(b: &[u8]) -> Result<ImageInfo, String> {
    let truncated = "WebP header is truncated";
    let (w, h) = match b.get(12..16) {
        Some(b"VP8X") => (
            le24(b, 24).ok_or(truncated)? + 1,
            le24(b, 27).ok_or(truncated)? + 1,
        ),
        Some(b"VP8L") => {
            if b.get(20) != Some(&0x2F) {
                return Err("WebP lossless signature is missing".into());
            }
            let bits = u32::from_le_bytes(b.get(21..25).ok_or(truncated)?.try_into().unwrap());
            ((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1)
        }
        Some(b"VP8 ") => {
            if b.get(23..26) != Some(&[0x9D, 0x01, 0x2A]) {
                return Err("WebP lossy start code is missing".into());
            }
            (
                le16(b, 26).ok_or(truncated)? & 0x3FFF,
                le16(b, 28).ok_or(truncated)? & 0x3FFF,
            )
        }
        _ => return Err("WebP has no VP8, VP8L or VP8X chunk".into()),
    };
    Ok(raster(ImageFormat::WebP, w, h))
}

fn probe_jpeg(b: &[u8]) -> Result<ImageInfo, String> {
    let mut i = 2;
    let mut adobe = false;
    let mut orientation = 1;
    loop {
        // Skip fill bytes before a marker.
        while b.get(i) == Some(&0xFF) && b.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if b.get(i) != Some(&0xFF) {
            return Err("JPEG is corrupt (marker expected)".into());
        }
        let marker = *b.get(i + 1).ok_or("JPEG is truncated")?;
        i += 2;
        // Markers without a length.
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            return Err("JPEG has no frame header before its image data".into());
        }
        let len = be16(b, i).ok_or("JPEG is truncated")? as usize;
        if len < 2 || i + len > b.len() {
            return Err("JPEG segment length is invalid".into());
        }
        let seg = &b[i + 2..i + len];
        match marker {
            // SOFn (not DHT/JPG/DAC).
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                if seg.len() < 6 {
                    return Err("JPEG frame header is truncated".into());
                }
                let h = u32::from(u16::from_be_bytes([seg[1], seg[2]]));
                let w = u32::from(u16::from_be_bytes([seg[3], seg[4]]));
                let components = seg[5];
                if !matches!(components, 1 | 3 | 4) {
                    return Err(format!("JPEG has {components} colour components"));
                }
                if seg[0] != 8 {
                    return Err(format!(
                        "JPEG has {}-bit samples; only 8 is supported",
                        seg[0]
                    ));
                }
                return Ok(ImageInfo {
                    format: ImageFormat::Jpeg,
                    width: w,
                    height: h,
                    components,
                    adobe,
                    orientation,
                });
            }
            0xEE if seg.starts_with(b"Adobe") => adobe = true,
            0xE1 if seg.starts_with(b"Exif\0\0") => {
                orientation = exif_orientation(&seg[6..]).unwrap_or(1);
            }
            _ => {}
        }
        i += len;
    }
}

/// Orientation tag (0x0112) of IFD0 in a TIFF-structured EXIF block.
fn exif_orientation(t: &[u8]) -> Option<u8> {
    let le = match t.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b = [*t.get(i)?, *t.get(i + 1)?];
        Some(if le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b: [u8; 4] = t.get(i..i + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    let count = u16_at(ifd)? as usize;
    for k in 0..count.min(256) {
        let e = ifd + 2 + k * 12;
        if u16_at(e)? == 0x0112 {
            let v = u16_at(e + 8)?;
            return (1..=8).contains(&v).then_some(v as u8);
        }
    }
    None
}
