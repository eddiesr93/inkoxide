//! Process-wide cache of decoded images, keyed by content hash (§7).
//!
//! An [`ImageStore`](super::ImageStore) already decodes each of its images
//! once, but the Node binding builds a fresh store per call from the
//! `images` option, so the same logo would be decoded and recompressed on
//! every render. This cache sits behind every store: least recently used
//! entries go once the decoded data exceeds [`BUDGET`].

use std::sync::{Arc, Mutex};

use super::decode::{DecodedImage, ImageBody, ImageDecoder};
use crate::lru::Lru;

/// Bytes (encoded plus compressed samples) kept across renders.
const BUDGET: usize = 64 << 20;

/// Encoded bytes (to rule out hash collisions) and the decoded image.
type Entry = (Arc<[u8]>, Arc<DecodedImage>);

static CACHE: Mutex<Option<Lru<u64, Entry>>> = Mutex::new(None);

fn size_of(image: &DecodedImage) -> usize {
    match &image.body {
        ImageBody::Jpeg { bytes, .. } => bytes.len(),
        ImageBody::Raw { samples, alpha, .. } => samples.len() + alpha.as_ref().map_or(0, Vec::len),
    }
}

/// Decodes `bytes` (whose xxh3 is `hash`), reusing an earlier decode of
/// the same content. Failures are not cached.
pub(crate) fn decode(
    bytes: &Arc<[u8]>,
    hash: u64,
    decoder: &dyn ImageDecoder,
) -> Result<Arc<DecodedImage>, String> {
    if let Ok(mut guard) = CACHE.lock() {
        let lru = guard.get_or_insert_with(|| Lru::new(BUDGET));
        if let Some((b, image)) = lru.get(&hash) {
            if b[..] == bytes[..] {
                return Ok(image);
            }
        }
    }
    // Decode outside the lock: other renders keep going meanwhile.
    let image = Arc::new(decoder.decode(bytes)?);
    if let Ok(mut guard) = CACHE.lock() {
        let size = size_of(&image) + bytes.len();
        guard.get_or_insert_with(|| Lru::new(BUDGET)).insert(
            hash,
            (bytes.clone(), image.clone()),
            size,
        );
    }
    Ok(image)
}

#[cfg(test)]
pub(crate) fn contains(hash: u64) -> bool {
    CACHE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|l| l.contains(&hash)))
        .unwrap_or(false)
}
