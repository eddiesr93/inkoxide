//! The images or the attachments of a call, as one buffer: their count,
//! then for each the length of its id, the id (UTF-8), the length of its
//! bytes and the bytes. Counts and lengths are little-endian `u32`s. An
//! empty buffer has none.

use inkoxide::{AttachmentStore, ImageStore};
use inkoxide_binding::Failure;

pub(crate) fn parse(buf: &[u8]) -> Result<ImageStore, Failure> {
    let mut store = ImageStore::new();
    for (id, bytes) in entries(buf, "options.images")? {
        store.insert(id, bytes.to_vec());
    }
    Ok(store)
}

pub(crate) fn parse_files(buf: &[u8]) -> Result<AttachmentStore, Failure> {
    let mut store = AttachmentStore::new();
    for (id, bytes) in entries(buf, "options.attachments")? {
        store.insert(id, bytes.to_vec());
    }
    Ok(store)
}

/// The ids and bytes of `buf`, the buffer of option `option`.
fn entries<'a>(buf: &'a [u8], option: &str) -> Result<Vec<(&'a str, &'a [u8])>, Failure> {
    let mut out = Vec::new();
    if buf.is_empty() {
        return Ok(out);
    }
    let mut r = Reader(buf);
    let count = r.u32()?;
    for _ in 0..count {
        let id = r.bytes()?;
        let id = std::str::from_utf8(id)
            .map_err(|_| Failure::invalid_arg(option, "an id is not UTF-8"))?;
        out.push((id, r.bytes()?));
    }
    if r.0.is_empty() {
        Ok(out)
    } else {
        Err(malformed())
    }
}

fn malformed() -> Failure {
    Failure::internal("the images or attachments buffer is malformed")
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Failure> {
        if n > self.0.len() {
            return Err(malformed());
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u32(&mut self) -> Result<u32, Failure> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A length, then that many bytes.
    fn bytes(&mut self) -> Result<&'a [u8], Failure> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}
