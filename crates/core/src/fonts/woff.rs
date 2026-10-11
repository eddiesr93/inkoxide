//! WOFF and WOFF2 containers, decoded to the sfnt (TrueType or CFF
//! OpenType) font they wrap before the font is parsed.
//!
//! WOFF compresses each table with zlib; WOFF2 compresses the whole font
//! with Brotli and stores `glyf`, `loca` (and optionally `hmtx`) in a
//! transformed form that `wuff` reconstructs. Decoding is a pure function
//! of the bytes, so a registered web font renders the same every time.

use std::io::Read;

/// Largest font a WOFF or WOFF2 file may decode to. `wuff` applies the
/// same bound to WOFF2 itself; WOFF tables are inflated against it here.
const MAX_SFNT_SIZE: usize = 128 << 20;

/// The sfnt inside a WOFF or WOFF2 file, or `None` when `bytes` is not in
/// either container (a plain font, or not a font at all).
///
/// # Errors
///
/// A message for a WOFF or WOFF2 file that cannot be decoded.
pub(super) fn decode(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    match bytes.get(..4) {
        Some(b"wOFF") => woff(bytes).map(Some),
        Some(b"wOF2") => wuff::decompress_woff2(bytes)
            .map(Some)
            .map_err(|_| "not a valid WOFF2 font".into()),
        _ => Ok(None),
    }
}

fn woff(bytes: &[u8]) -> Result<Vec<u8>, String> {
    // Uncompressed tables are copied from `bytes`; compressed ones count
    // against the budget before anything is allocated for them.
    let mut budget = MAX_SFNT_SIZE;
    let mut too_large = false;
    let decoded = wuff::decompress_woff1_with_custom_z(bytes, &mut |table, len| {
        if len > budget {
            too_large = true;
            return Err("table exceeds the size limit".into());
        }
        budget -= len;
        let mut out = Vec::with_capacity(len);
        // One byte more than declared, so that a longer stream fails
        // wuff's length check instead of being cut silently.
        flate2::read::ZlibDecoder::new(table)
            .take(len as u64 + 1)
            .read_to_end(&mut out)?;
        Ok(out)
    });
    decoded.map_err(|_| {
        if too_large {
            format!("WOFF font decodes to more than {} MiB", MAX_SFNT_SIZE >> 20)
        } else {
            "not a valid WOFF font".into()
        }
    })
}

/// A WOFF file holding the tables of `sfnt`, zlib-compressed when
/// `compress` is set; `orig_len` overrides each table's declared size.
#[cfg(test)]
pub(super) fn woff_from_sfnt(sfnt: &[u8], compress: bool, orig_len: Option<u32>) -> Vec<u8> {
    let face = ttf_parser::RawFace::parse(sfnt, 0).unwrap();
    let tables: Vec<_> = face.table_records.into_iter().collect();
    let start = 44 + 20 * tables.len();
    let mut dir = Vec::new();
    let mut data = Vec::new();
    for t in &tables {
        let raw = &sfnt[t.offset as usize..(t.offset + t.length) as usize];
        // Stored as is when compression does not make the table smaller.
        let stored = Some(crate::pdf::deflate(raw, 9).unwrap())
            .filter(|z| compress && z.len() < raw.len())
            .unwrap_or_else(|| raw.to_vec());
        dir.extend_from_slice(&t.tag.to_bytes());
        dir.extend_from_slice(&((start + data.len()) as u32).to_be_bytes());
        dir.extend_from_slice(&(stored.len() as u32).to_be_bytes());
        dir.extend_from_slice(&orig_len.unwrap_or(t.length).to_be_bytes());
        dir.extend_from_slice(&t.check_sum.to_be_bytes());
        data.extend_from_slice(&stored);
        data.resize(data.len().next_multiple_of(4), 0);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&sfnt[..4]); // flavor
    out.extend_from_slice(&((start + data.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0; 2]); // reserved
    out.extend_from_slice(&(sfnt.len() as u32).to_be_bytes()); // totalSfntSize
    out.extend_from_slice(&[0, 1, 0, 0]); // version 1.0
    out.extend_from_slice(&[0; 20]); // no metadata or private data
    out.extend_from_slice(&dir);
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_fonts_pass_through() {
        assert_eq!(decode(b"\0\x01\0\0rest"), Ok(None));
        assert_eq!(decode(b"OTTO"), Ok(None));
        assert_eq!(decode(b""), Ok(None));
    }

    #[test]
    fn truncated_containers_fail() {
        assert_eq!(decode(b"wOFF").unwrap_err(), "not a valid WOFF font");
        assert_eq!(decode(b"wOF2\0\0").unwrap_err(), "not a valid WOFF2 font");
    }

    #[test]
    fn woff_tables_inflate_within_the_limit() {
        let sfnt = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/fonts/NotoSansHebrew-Regular.ttf"
        ))
        .unwrap();
        assert!(decode(&woff_from_sfnt(&sfnt, true, None))
            .unwrap()
            .is_some());
        // Every table claims 4 GiB: rejected before anything is allocated.
        let bomb = woff_from_sfnt(&sfnt, true, Some(u32::MAX));
        assert_eq!(
            decode(&bomb).unwrap_err(),
            "WOFF font decodes to more than 128 MiB"
        );
        // Tables that inflate to less than they declare.
        let short = woff_from_sfnt(&sfnt, true, Some(1 << 20));
        assert_eq!(decode(&short).unwrap_err(), "not a valid WOFF font");
    }
}
