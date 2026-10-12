//! Header inspection of JPEG, PNG, GIF and WebP files (SVG is parsed): format, pixel size,
//! components and EXIF orientation. Cheap, no decode: validation runs it
//! before decoding, and the JPEG passthrough path relies on it (for JPEG it
//! also checks the marker structure, as JPEG is never decoded).

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
    /// JPEG colour components (1, 3 or 4); 0 for the other formats.
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

/// The format `bytes` start like: the signature of a raster format, or
/// what looks like SVG markup. Nothing past the signature is checked.
pub(crate) fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(PNG_SIGNATURE) {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(ImageFormat::WebP)
    } else if super::svg::is_svg(bytes) {
        Some(ImageFormat::Svg)
    } else {
        None
    }
}

pub(crate) fn probe(bytes: &[u8]) -> Result<ImageInfo, ProbeError> {
    let info = match sniff(bytes) {
        Some(ImageFormat::Png) => probe_png(bytes)?,
        Some(ImageFormat::Jpeg) => probe_jpeg(bytes)?,
        Some(ImageFormat::Gif) => probe_gif(bytes)?,
        Some(ImageFormat::WebP) => probe_webp(bytes)?,
        Some(ImageFormat::Svg) => {
            // No header to read: parsing is the only real check.
            let size = super::svg::parse(bytes)?.0.size();
            return Ok(raster(
                ImageFormat::Svg,
                size.width().ceil() as u32,
                size.height().ceil() as u32,
            ));
        }
        None => {
            return Err(ProbeError::Invalid(
                "unsupported image format (expected JPEG, PNG, GIF, WebP or SVG)".into(),
            ))
        }
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
    Ok(raster(ImageFormat::Png, w, h))
}

/// The header of an image other than a JPEG: no colour components to
/// report and no EXIF orientation.
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

/// Reads the frame header and walks the whole marker structure, so that a
/// file cut short (no end-of-image marker) or coded in a way `DCTDecode`
/// cannot show is rejected. The compressed image data itself is not
/// decoded: it goes into the PDF as it is.
fn probe_jpeg(b: &[u8]) -> Result<ImageInfo, String> {
    let mut i = 2;
    let mut adobe = false;
    let mut orientation = 1;
    let mut frame = None;
    let mut scanned = false;
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
        if frame.is_none() && (marker == 0xD9 || marker == 0xDA) {
            return Err("JPEG has no frame header before its image data".into());
        }
        if marker == 0xD9 {
            if !scanned {
                return Err("JPEG has no image data".into());
            }
            let (width, height, components) = frame.unwrap_or_default();
            return Ok(ImageInfo {
                format: ImageFormat::Jpeg,
                width,
                height,
                components,
                adobe,
                orientation,
            });
        }
        let len = be16(b, i).ok_or("JPEG is truncated")? as usize;
        if len < 2 || i + len > b.len() {
            return Err("JPEG segment length is invalid".into());
        }
        let seg = &b[i + 2..i + len];
        match marker {
            // Baseline, extended and progressive Huffman coding: what
            // `DCTDecode` reads.
            0xC0..=0xC2 => {
                if frame.is_some() {
                    return Err("JPEG has more than one frame header".into());
                }
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
                frame = Some((w, h, components));
            }
            // The other SOFn (not DHT/JPG/DAC): lossless, hierarchical or
            // arithmetic coding.
            0xC3..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Err(
                    "JPEG uses lossless, hierarchical or arithmetic coding; only baseline and progressive JPEGs are supported"
                        .into(),
                );
            }
            0xEE if seg.starts_with(b"Adobe") => adobe = true,
            0xE1 if seg.starts_with(b"Exif\0\0") => {
                orientation = exif_orientation(&seg[6..]).unwrap_or(1);
            }
            _ => {}
        }
        i += len;
        if marker == 0xDA {
            i = scan_end(b, i)?;
            scanned = true;
        }
    }
}

/// Where the compressed data of a scan that starts at `i` ends: at the
/// first marker other than a restart marker.
fn scan_end(b: &[u8], mut i: usize) -> Result<usize, String> {
    const TRUNCATED: &str = "JPEG is truncated (its image data has no end marker)";
    loop {
        i += b[i..].iter().position(|&x| x == 0xFF).ok_or(TRUNCATED)?;
        match b.get(i + 1) {
            None => return Err(TRUNCATED.into()),
            // A stuffed 0xFF byte or a restart marker.
            Some(0x00 | 0xD0..=0xD7) => i += 2,
            // Fill bytes before a marker.
            Some(0xFF) => i += 1,
            Some(_) => return Ok(i),
        }
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
