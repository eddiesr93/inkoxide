//! Flate (zlib) compression, for the PDF writer and for the image
//! decoder, which keeps decoded samples compressed. Neither depends on the
//! other for it.

use std::io::{Read as _, Write as _};

use flate2::read::ZlibDecoder;
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

/// The `len` bytes `data` holds, which [`deflate`] compressed: data that
/// holds another length is an error.
pub(crate) fn inflate(data: &[u8], len: usize) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    ZlibDecoder::new(data)
        .take(len as u64 + 1)
        .read_to_end(&mut out)?;
    if out.len() == len {
        Ok(out)
    } else {
        Err(std::io::Error::other(format!(
            "{} bytes where {len} were compressed",
            out.len()
        )))
    }
}
