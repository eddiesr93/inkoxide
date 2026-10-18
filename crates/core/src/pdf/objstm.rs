//! Objects packed into object streams (ISO 32000-1, 7.5.7): the structure
//! elements and parent tree of tagged output, many small dictionaries that
//! compress well together and otherwise cost a line of cross-reference
//! table each. A file with object streams ends in a cross-reference stream
//! (7.5.8), which pdf-writer writes with its own entries only, so the
//! entries of the packed objects are filled in here.

use std::fmt::Write as _;

use pdf_writer::{Filter, Name, Pdf, Ref, XRefFilter};

use super::out::Out;
use super::RefAlloc;
use crate::InkoxideError;

/// Objects per stream: a reader inflates a whole stream to read one.
const PER_STREAM: usize = 128;

/// The objects bound for object streams, as pdf-writer wrote them.
pub(super) struct Packed {
    /// Flate level of the streams.
    level: u32,
    /// Each object and its value, without `obj` and `endobj`.
    objects: Vec<(Ref, Vec<u8>)>,
}

impl Packed {
    pub fn new(level: u32) -> Self {
        Self {
            level,
            objects: Vec::new(),
        }
    }

    /// Adds `chunk`, which holds indirect object `r` alone.
    pub fn push(&mut self, r: Ref, chunk: &[u8]) {
        let head = format!("{} 0 obj\n", r.get());
        let value = chunk
            .strip_prefix(head.as_bytes())
            .and_then(|b| b.strip_suffix(b"\nendobj\n\n"))
            .expect("an indirect object as pdf-writer writes it");
        self.objects.push((r, value.to_vec()));
    }

    /// Writes the object streams into `pdf`, then its cross-reference
    /// stream and trailer.
    pub fn finish(
        self,
        mut pdf: Pdf,
        alloc: &mut RefAlloc,
        out: &mut Out,
    ) -> Result<Vec<u8>, InkoxideError> {
        if self.objects.is_empty() {
            return Ok(pdf.finish());
        }
        // Each stream: the number and offset of each object, then the
        // objects (7.5.7).
        let streams: Vec<(Vec<u8>, usize, usize)> = self
            .objects
            .chunks(PER_STREAM)
            .map(|group| {
                let mut head = String::new();
                let mut body = Vec::new();
                for (r, value) in group {
                    let _ = write!(head, "{} {} ", r.get(), body.len());
                    body.extend_from_slice(value);
                    body.push(b'\n');
                }
                let first = head.len();
                let mut data = head.into_bytes();
                data.extend(body);
                (data, first, group.len())
            })
            .collect();
        let compressed = streams
            .iter()
            .map(|(data, ..)| super::deflate(data, self.level))
            .collect::<Result<Vec<_>, _>>()?;
        // The cross-reference stream's fields are as wide as its own
        // offset needs, and must hold every object number: the file is at
        // least as long as the largest number when every object takes a
        // byte, which the streams do uncompressed.
        let refs = streams.len() + 1;
        let largest = i64::from(alloc.0) + refs as i64;
        let long = pdf.len() + compressed.iter().map(Vec::len).sum::<usize>();
        let deflated = i64::try_from(long).unwrap_or(i64::MAX) >= largest;
        let mut entries = vec![None; usize::try_from(largest).unwrap_or(0) + 1];
        for (i, ((data, first, n), packed)) in streams.iter().zip(&compressed).enumerate() {
            let r = alloc.next();
            for (index, (o, _)) in self.objects[i * PER_STREAM..][..*n].iter().enumerate() {
                entries[o.get() as usize] = Some((r.get(), index as u16));
            }
            let data = if deflated { packed } else { data };
            out.stream(&mut pdf, r, data, |s| {
                s.pair(Name(b"Type"), Name(b"ObjStm"))
                    .pair(Name(b"N"), *n as i32)
                    .pair(Name(b"First"), *first as i32);
                if deflated {
                    s.filter(Filter::FlateDecode);
                }
            });
        }
        let xref = alloc.next();
        let level = self.level;
        let mut failed = None;
        let file = pdf.finish_with_xref_stream_and_filter(xref, |raw| {
            let entries = &entries[..=xref.get() as usize];
            let mut raw = raw.to_vec();
            fill_entries(&mut raw, entries);
            let data = super::deflate(&raw, level).unwrap_or_else(|e| {
                failed = Some(e);
                Vec::new()
            });
            (data, XRefFilter::Single(Filter::FlateDecode))
        });
        match failed {
            Some(e) => Err(e),
            None => Ok(file),
        }
    }
}

/// Makes the entries of `raw`, a cross-reference stream as pdf-writer
/// wrote it (fields 1, n and 2 bytes wide, one entry per object number),
/// those of the packed objects in `packed`: type 2, the stream and the
/// index in it. pdf-writer had them free, so the list of free entries is
/// linked again without them.
fn fill_entries(raw: &mut [u8], packed: &[Option<(i32, u16)>]) {
    let size = raw.len() / packed.len();
    let width = size - 3;
    let put = |entry: &mut [u8], kind: u8, field: u64, last: u16| {
        entry[0] = kind;
        entry[1..=width].copy_from_slice(&field.to_be_bytes()[8 - width..]);
        entry[size - 2..].copy_from_slice(&last.to_be_bytes());
    };
    for (entry, p) in raw.chunks_exact_mut(size).zip(packed) {
        if let &Some((stream, index)) = p {
            put(entry, 2, stream as u64, index);
        }
    }
    let free: Vec<usize> = (0..packed.len()).filter(|&i| raw[i * size] == 0).collect();
    for (k, &i) in free.iter().enumerate() {
        let next = free.get(k + 1).copied().unwrap_or(0);
        let generation = if i == 0 { u16::MAX } else { 0 };
        put(&mut raw[i * size..][..size], 0, next as u64, generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_entries_point_into_their_stream() {
        // Objects 0 (free), 1 (in the file), 2 (packed), 3 (free), 4 (in
        // the file), fields 2 bytes wide.
        let mut raw = vec![
            0, 0, 3, 0xFF, 0xFF, //
            1, 0, 10, 0, 0, //
            0, 0, 3, 0, 0, //
            0, 0, 0, 0, 0, //
            1, 0, 99, 0, 0,
        ];
        fill_entries(&mut raw, &[None, None, Some((4, 7)), None, None]);
        assert_eq!(
            raw,
            [
                0, 0, 3, 0xFF, 0xFF, //
                1, 0, 10, 0, 0, //
                2, 0, 4, 0, 7, //
                0, 0, 0, 0, 0, //
                1, 0, 99, 0, 0,
            ]
        );
    }
}
