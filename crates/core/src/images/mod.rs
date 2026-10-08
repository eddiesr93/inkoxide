//! Image registry. Decoding and PDF XObjects arrive in M5.

use std::collections::BTreeMap;
use std::sync::Arc;

/// Encoded image bytes (JPEG or PNG) available to a render, keyed by the id
/// that `image` nodes reference in `src`.
#[derive(Debug, Clone, Default)]
pub struct ImageStore {
    images: BTreeMap<String, Arc<[u8]>>,
}

impl ImageStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.images.insert(id.into(), bytes.into());
    }

    pub fn contains(&self, id: &str) -> bool {
        self.images.contains_key(id)
    }

    #[allow(dead_code)]
    pub(crate) fn get(&self, id: &str) -> Option<&Arc<[u8]>> {
        self.images.get(id)
    }
}
