//! Process-wide cache of decoded images, keyed by content.
//!
//! An [`ImageStore`](super::ImageStore) already decodes each of its images
//! once, but the Node binding builds a fresh store per call from the
//! `images` option, so the same logo would be decoded and recompressed on
//! every render. This cache sits behind every store: least recently used
//! entries go once the decoded data exceeds [`BUDGET`].

use std::sync::Arc;

use super::decode::{DecodedImage, ImageBody, ImageDecoder};
use super::ContentKey;
use crate::lru::SharedLru;

/// Bytes (encoded plus compressed samples) kept across renders.
const BUDGET: usize = 64 << 20;

static CACHE: SharedLru<ContentKey<Arc<[u8]>>, Arc<DecodedImage>> = SharedLru::new(BUDGET);

/// Approximate memory held by an entry whose encoded form is `encoded`
/// bytes long.
fn size_of(image: &DecodedImage, encoded: usize) -> usize {
    match &image.body {
        ImageBody::Jpeg { bytes, .. } => bytes.len() + encoded,
        ImageBody::Raw { samples, alpha, .. } => {
            samples.len() + alpha.as_ref().map_or(0, Vec::len) + encoded
        }
        // A parsed tree is several times larger than its source.
        ImageBody::Svg { .. } => encoded * 8,
    }
}

/// Decodes `bytes` (whose xxh3 is `hash`), reusing an earlier decode of
/// the same content. Failures are not cached.
pub(crate) fn decode(
    bytes: &Arc<[u8]>,
    hash: u64,
    decoder: &dyn ImageDecoder,
) -> Result<Arc<DecodedImage>, String> {
    let key = ContentKey::with_hash(bytes.clone(), hash);
    if let Some(image) = CACHE.get(&key) {
        return Ok(image);
    }
    // Decode outside the lock: other renders keep going meanwhile.
    let image = Arc::new(decoder.decode(bytes)?);
    CACHE.insert(key, image.clone(), size_of(&image, bytes.len()));
    Ok(image)
}

#[cfg(test)]
pub(crate) fn contains(bytes: &Arc<[u8]>, hash: u64) -> bool {
    CACHE.contains(&ContentKey::with_hash(bytes.clone(), hash))
}
