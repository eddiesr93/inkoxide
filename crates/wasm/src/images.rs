//! The images of a call, as one buffer: their count, then for each image
//! the length of its id, the id (UTF-8), the length of its bytes and the
//! bytes. Counts and lengths are little-endian `u32`s. An empty buffer
//! has no images.

use inkoxide::ImageStore;
use inkoxide_binding::Failure;

pub(crate) fn parse(buf: &[u8]) -> Result<ImageStore, Failure> {
    let mut store = ImageStore::new();
    if buf.is_empty() {
        return Ok(store);
    }
    let mut r = Reader(buf);
    let count = r.u32()?;
    for _ in 0..count {
        let id = r.bytes()?;
        let id = std::str::from_utf8(id)
            .map_err(|_| Failure::invalid_arg("options.images", "an id is not UTF-8"))?;
        store.insert(id, r.bytes()?.to_vec());
    }
    if r.0.is_empty() {
        Ok(store)
    } else {
        Err(malformed())
    }
}

fn malformed() -> Failure {
    Failure::internal("the images buffer is malformed")
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
