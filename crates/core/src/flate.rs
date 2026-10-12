//! Flate (zlib) compression, for the PDF writer and for the image
//! decoder, which keeps decoded samples compressed. Neither depends on the
//! other for it.

use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::Compression;

/// Level for embedded objects (font programs, image samples, SVG forms):
/// fonts and images are compressed once and cached, so favour size.
pub(crate) const LEVEL: u32 = 6;

pub(crate) fn deflate(data: &[u8], level: u32) -> std::io::Result<Vec<u8>> {
    let mut enc = ZlibEncoder::new(
        Vec::with_capacity(data.len() / 3 + 64),
        Compression::new(level),
    );
    enc.write_all(data)?;
    enc.finish()
}
