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

    /// An upper bound of the decoded size, before decoding: the whitespace
    /// the decoder skips (base64 in lines) is left out.
    pub(crate) fn max_len(&self) -> usize {
        let digits = self
            .b64
            .bytes()
            .filter(|b| !b.is_ascii_whitespace())
            .count();
        digits / 4 * 3 + 3
    }
}

/// Files available to a render, keyed by the id that `attachments[].src`
/// names. Cloning is cheap: the bytes are shared.
#[derive(Debug, Clone, Default)]
pub struct AttachmentStore {
    files: BTreeMap<String, StoredFile>,
}

/// A file of an [`AttachmentStore`]: its bytes, or only the size of one
/// too large to read.
#[derive(Debug, Clone)]
enum StoredFile {
    Bytes(Arc<[u8]>),
    TooLarge(u64),
}

impl AttachmentStore {
    /// The most bytes of one file a document may use (64 MiB).
    pub const MAX_FILE_BYTES: usize = MAX_ATTACHMENT_BYTES;

    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) file `id`. Its size is checked when a document
    /// uses it (`ATTACHMENT_TOO_LARGE`).
    pub fn insert(&mut self, id: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.files
            .insert(id.into(), StoredFile::Bytes(bytes.into()));
    }

    /// Adds (or replaces) file `id` by its size alone, `len` bytes, for a
    /// file over an attachment's limit (64 MiB), not worth reading: a
    /// document that uses it fails with `ATTACHMENT_TOO_LARGE`, as it would
    /// with the bytes.
    pub fn insert_too_large(&mut self, id: impl Into<String>, len: u64) {
        let len = len.max(Self::MAX_FILE_BYTES as u64 + 1);
        self.files.insert(id.into(), StoredFile::TooLarge(len));
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

    /// The bytes of file `id`; none for one too large to read.
    pub(crate) fn get(&self, id: &str) -> Option<&Arc<[u8]>> {
        match self.files.get(id)? {
            StoredFile::Bytes(bytes) => Some(bytes),
            StoredFile::TooLarge(_) => None,
        }
    }

    /// The size of file `id`, whether its bytes were read or not.
    pub(crate) fn len_of(&self, id: &str) -> Option<u64> {
        Some(match self.files.get(id)? {
            StoredFile::Bytes(bytes) => bytes.len() as u64,
            StoredFile::TooLarge(len) => *len,
        })
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
/// are) when that makes it smaller, in `parallel` if asked; dates default
/// to `created`.
pub(crate) fn prepare(
    decls: &[AttachmentDecl],
    store: &AttachmentStore,
    level: Option<u32>,
    created: Option<Timestamp>,
    parallel: bool,
) -> Result<Vec<PreparedFile>, InkoxideError> {
    let one = |d: &AttachmentDecl| {
        let bytes = bytes_of(d, store).ok_or_else(|| {
            InkoxideError::Internal(format!("attachment `{}` vanished after validation", d.name))
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
    };
    if parallel {
        use rayon::prelude::*;
        decls.par_iter().map(one).collect()
    } else {
        decls.iter().map(one).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(name: &str, source: FileSource, modified: Option<&str>) -> AttachmentDecl {
        AttachmentDecl {
            name: name.into(),
            source,
            mime_type: None,
            description: None,
            relationship: Relationship::Data,
            modified: modified.map(Into::into),
        }
    }

    #[test]
    fn prepare_compresses_only_what_gets_smaller() {
        let mut store = AttachmentStore::new();
        store.insert("zeros", vec![0u8; 4096]);
        let created = crate::date::parse_rfc3339("2026-10-08T00:00:00Z");
        let decls = [
            decl("zeros.bin", FileSource::Ref("zeros".into()), None),
            decl(
                "hi.txt",
                FileSource::Inline(InlineFile::new("aGk=".into())),
                Some("2026-01-02T03:04:05+02:00"),
            ),
        ];
        let files = prepare(&decls, &store, Some(6), created, true).unwrap();
        let [zeros, hi] = &files[..] else { panic!() };
        assert!(zeros.compressed && zeros.data.len() < 4096);
        assert_eq!(zeros.size, 4096);
        assert_eq!(zeros.mime_type, DEFAULT_MIME_TYPE);
        assert_eq!(zeros.modified, created);
        // Two bytes deflate to more than two.
        assert!(!hi.compressed);
        assert_eq!((&hi.data[..], hi.size), (&b"hi"[..], 2));
        assert_eq!(hi.modified.unwrap().offset_minutes, 120);
        assert_ne!(zeros.hash, hi.hash);
        // Without compression, every file is stored as it is.
        let stored = prepare(&decls, &store, None, None, false).unwrap();
        assert!(stored
            .iter()
            .all(|f| !f.compressed && f.data.len() == f.size));
        assert_eq!(stored[0].modified, None);
    }

    #[test]
    fn prepare_reports_a_file_missing_from_the_store() {
        let decls = [decl("gone.bin", FileSource::Ref("gone".into()), None)];
        let e = prepare(&decls, &AttachmentStore::new(), None, None, false).unwrap_err();
        assert!(matches!(e, InkoxideError::Internal(_)), "{e}");
    }

    #[test]
    fn inline_files_decode_once_within_their_bound() {
        for (b64, bytes) in [("", &b""[..]), ("aGk=", b"hi"), ("aGVsbG8=", b"hello")] {
            let f = InlineFile::new(b64.into());
            assert_eq!(&f.bytes().unwrap()[..], bytes);
            assert!(f.max_len() >= bytes.len(), "{b64}");
        }
        // Base64 in lines: the bound counts the digits, not the breaks.
        let lines = InlineFile::new(format!("{}\r\n", "QUFB".repeat(19)).repeat(100));
        assert_eq!(lines.bytes().unwrap().len(), 5700);
        assert_eq!(lines.max_len(), 5703);
        assert!(InlineFile::new("!!".into()).bytes().is_err());
    }
}
