//! Files a PDF carries: their bytes (a store passed to the render, or
//! base64 in the document), and the form the writer embeds them in.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use crate::date::Timestamp;
use crate::schema::{AttachmentDecl, Relationship};
use crate::InkoxideError;

/// Most files a document declares.
pub(crate) const MAX_ATTACHMENTS: usize = 1000;
/// Most bytes of one file.
pub(crate) const MAX_ATTACHMENT_BYTES: usize = 64 << 20;
/// Most bytes of a document's files together: a memory guard, as an
/// encrypted render holds them twice.
pub(crate) const MAX_ATTACHMENTS_TOTAL: usize = 256 << 20;
/// Longest file name, in characters.
pub(crate) const MAX_FILE_NAME: usize = 255;
/// Longest MIME type, in bytes: the PDF name it becomes (a PDF/A-1 limit).
pub(crate) const MAX_MIME_TYPE: usize = 127;

/// The MIME type of a file declared without one.
pub(crate) const DEFAULT_MIME_TYPE: &str = "application/octet-stream";

/// Where an attachment's bytes come from.
#[derive(Debug, Clone)]
pub enum FileSource {
    /// An id of the render's [`AttachmentStore`].
    Ref(String),
    /// Base64 `data` in the document.
    Inline(InlineFile),
}

/// The base64 `data` of an attachment, decoded once, on first use.
#[derive(Debug, Clone)]
pub struct InlineFile {
    b64: String,
    bytes: OnceLock<Result<Arc<[u8]>, String>>,
}

impl InlineFile {
    pub(crate) fn new(b64: String) -> Self {
        Self {
            b64,
            bytes: OnceLock::new(),
        }
    }

    /// The decoded bytes, or why the payload is not base64.
    pub(crate) fn bytes(&self) -> Result<&Arc<[u8]>, &str> {
        self.bytes
            .get_or_init(|| crate::images::base64::decode(&self.b64).map(Into::into))
            .as_ref()
            .map_err(String::as_str)
    }

    /// An upper bound of the decoded size, before decoding.
    pub(crate) fn max_len(&self) -> usize {
        self.b64.len() / 4 * 3 + 3
    }
}

/// Files available to a render, keyed by the id that `attachments[].src`
/// names. Cloning is cheap: the bytes are shared.
#[derive(Debug, Clone, Default)]
pub struct AttachmentStore {
    files: BTreeMap<String, Arc<[u8]>>,
}

impl AttachmentStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) file `id`. Its size is checked when a document
    /// uses it (`ATTACHMENT_TOO_LARGE`).
    pub fn insert(&mut self, id: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.files.insert(id.into(), bytes.into());
    }

    /// Removes file `id`; returns whether it was present.
    pub fn remove(&mut self, id: &str) -> bool {
        self.files.remove(id).is_some()
    }

    /// Whether file `id` is present.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.files.contains_key(id)
    }

    /// Number of files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the store holds no file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub(crate) fn get(&self, id: &str) -> Option<&Arc<[u8]>> {
        self.files.get(id)
    }
}

/// The bytes of `decl`: from `store`, or its own `data`. Validation has
/// checked that they exist and decode.
pub(crate) fn bytes_of<'a>(
    decl: &'a AttachmentDecl,
    store: &'a AttachmentStore,
) -> Option<&'a Arc<[u8]>> {
    match &decl.source {
        FileSource::Ref(id) => store.get(id),
        FileSource::Inline(inline) => inline.bytes().ok(),
    }
}

/// A file as the writer embeds it: compressed when that is smaller.
#[derive(Debug)]
pub(crate) struct PreparedFile {
    pub name: String,
    pub mime_type: String,
    pub description: Option<String>,
    pub relationship: Relationship,
    pub modified: Option<Timestamp>,
    /// The stream data: Flate-compressed when `compressed`.
    pub data: Vec<u8>,
    pub compressed: bool,
    /// The size of the file itself.
    pub size: usize,
    /// A hash of the file, for the PDF/A file identifier.
    pub hash: u64,
}

/// The files of `decls`, each compressed at `level` (none: stored as they
/// are) when that makes it smaller; dates default to `created`.
pub(crate) fn prepare(
    decls: &[AttachmentDecl],
    store: &AttachmentStore,
    level: Option<u32>,
    created: Option<Timestamp>,
) -> Result<Vec<PreparedFile>, InkoxideError> {
    decls
        .iter()
        .map(|d| {
            let bytes = bytes_of(d, store).ok_or_else(|| {
                InkoxideError::Internal(format!(
                    "attachment `{}` vanished after validation",
                    d.name
                ))
            })?;
            let packed = match level {
                Some(level) => Some(
                    crate::flate::deflate(bytes, level)
                        .map_err(|e| InkoxideError::Pdf(format!("compression failed: {e}")))?,
                )
                .filter(|p| p.len() < bytes.len()),
                None => None,
            };
            let compressed = packed.is_some();
            Ok(PreparedFile {
                name: d.name.clone(),
                mime_type: d
                    .mime_type
                    .clone()
                    .unwrap_or_else(|| DEFAULT_MIME_TYPE.into()),
                description: d.description.clone(),
                relationship: d.relationship,
                modified: d
                    .modified
                    .as_deref()
                    .and_then(crate::date::parse_rfc3339)
                    .or(created),
                data: packed.unwrap_or_else(|| bytes.to_vec()),
                compressed,
                size: bytes.len(),
                hash: xxhash_rust::xxh3::xxh3_64(bytes),
            })
        })
        .collect()
}
