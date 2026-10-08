//! Font registry. M0 keeps raw bytes; parsing, metrics and PDF embedding
//! arrive in M2.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::InkoxideError;

/// A registered font face.
#[derive(Debug)]
pub struct FontData {
    pub(crate) bytes: Arc<[u8]>,
}

/// Fonts available to a render, keyed by the id documents reference.
///
/// Cheap to clone: faces are shared behind `Arc`.
#[derive(Debug, Clone, Default)]
pub struct FontStore {
    fonts: BTreeMap<String, Arc<FontData>>,
}

impl FontStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) font `id` from TrueType/OpenType bytes.
    pub fn register(
        &mut self,
        id: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<(), InkoxideError> {
        let id = id.into();
        let bytes = bytes.into();
        if bytes.len() < 12 {
            return Err(InkoxideError::Font(format!(
                "font `{id}`: data is too short to be a font"
            )));
        }
        self.fonts.insert(id, Arc::new(FontData { bytes }));
        Ok(())
    }

    /// Removes font `id`; returns whether it was registered.
    pub fn unregister(&mut self, id: &str) -> bool {
        self.fonts.remove(id).is_some()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.fonts.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    #[allow(dead_code)]
    pub(crate) fn get(&self, id: &str) -> Option<&Arc<FontData>> {
        self.fonts.get(id)
    }
}
