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
    let too_large = || format!("WOFF font decodes to more than {} MiB", MAX_SFNT_SIZE >> 20);
    // Every table counts at its declared size before anything is decoded:
    // stored tables are copied from `bytes`, and several entries of the
    // directory may point at the same bytes.
    if declared_size(bytes).is_some_and(|n| n > MAX_SFNT_SIZE as u64) {
        return Err(too_large());
    }
    let decoded = wuff::decompress_woff1_with_custom_z(bytes, &mut |table, len| {
        let mut out = Vec::with_capacity(len);
        // One byte more than declared, so that a longer stream fails
        // wuff's length check instead of being cut silently.
        flate2::read::ZlibDecoder::new(table)
            .take(len as u64 + 1)
            .read_to_end(&mut out)?;
        Ok(out)
    });
    decoded.map_err(|_| "not a valid WOFF font".into())
}

/// The sum of the sizes a WOFF file's tables declare (`origLength`), or
/// `None` when its directory is cut short (`wuff` then refuses the file).
fn declared_size(bytes: &[u8]) -> Option<u64> {
    let be_u32 = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let tables = u16::from_be_bytes(bytes.get(12..14)?.try_into().ok()?);
    (0..usize::from(tables)).try_fold(0u64, |sum, i| {
        Some(sum + u64::from(be_u32(44 + 20 * i + 12)?))
    })
}

/// A WOFF file holding the tables of `sfnt`, zlib-compressed when
/// `compress` is set; `orig_len` overrides each table's declared size.
#[cfg(test)]
pub(super) fn woff_from_sfnt(sfnt: &[u8], compress: bool, orig_len: Option<u32>) -> Vec<u8> {
    let font = read_fonts::FontRef::new(sfnt).unwrap();
    let tables = font.table_directory().table_records();
    let start = 44 + 20 * tables.len();
    let mut dir = Vec::new();
    let mut data = Vec::new();
    for t in tables {
        let (offset, length) = (t.offset() as usize, t.length());
        let raw = &sfnt[offset..offset + length as usize];
        // Stored as is when compression does not make the table smaller.
        let mut z = Vec::new();
        flate2::read::ZlibEncoder::new(raw, flate2::Compression::best())
            .read_to_end(&mut z)
            .unwrap();
        let stored = Some(z)
            .filter(|z| compress && z.len() < raw.len())
            .unwrap_or_else(|| raw.to_vec());
        dir.extend_from_slice(&t.tag().to_be_bytes());
        dir.extend_from_slice(&((start + data.len()) as u32).to_be_bytes());
        dir.extend_from_slice(&(stored.len() as u32).to_be_bytes());
        dir.extend_from_slice(&orig_len.unwrap_or(length).to_be_bytes());
        dir.extend_from_slice(&t.checksum().to_be_bytes());
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
        let sfnt = crate::test_fixtures::fixture("fonts/NotoSansHebrew-Regular.ttf");
        assert!(decode(&woff_from_sfnt(&sfnt, true, None))
            .unwrap()
            .is_some());
        // Every table claims 4 GiB: rejected before anything is allocated.
        let bomb = woff_from_sfnt(&sfnt, true, Some(u32::MAX));
        assert_eq!(
            decode(&bomb).unwrap_err(),
            "WOFF font decodes to more than 128 MiB"
        );
        // Stored tables that each fit, but whose entries all point at the
        // same bytes: 2,100 copies of 64 KiB add up to more than 128 MiB.
        let block = 64 << 10;
        let copies = 2100;
        let start = 44 + 20 * copies;
        let mut shared = Vec::new();
        shared.extend_from_slice(b"wOFF\0\x01\0\0");
        shared.extend_from_slice(&((start + block) as u32).to_be_bytes());
        shared.extend_from_slice(&(copies as u16).to_be_bytes());
        shared.extend_from_slice(&[0; 2]);
        shared.extend_from_slice(&u32::MAX.to_be_bytes()); // totalSfntSize
        shared.extend_from_slice(&[0, 1, 0, 0]);
        shared.extend_from_slice(&[0; 20]);
        for i in 0..copies {
            shared.extend_from_slice(&(i as u32).to_be_bytes()); // tag
            shared.extend_from_slice(&(start as u32).to_be_bytes());
            shared.extend_from_slice(&(block as u32).to_be_bytes());
            shared.extend_from_slice(&(block as u32).to_be_bytes());
            shared.extend_from_slice(&[0; 4]);
        }
        shared.resize(start + block, 0);
        assert_eq!(
            decode(&shared).unwrap_err(),
            "WOFF font decodes to more than 128 MiB"
        );
        // Tables that inflate to less than they declare.
        let short = woff_from_sfnt(&sfnt, true, Some(1 << 20));
        assert_eq!(decode(&short).unwrap_err(), "not a valid WOFF font");
    }

    /// 1 KiB of zeros, Brotli-compressed.
    const ZEROS_1_KIB: &[u8] = &[
        0x1b, 0xff, 0x03, 0x00, 0x24, 0x00, 0xa2, 0xb1, 0x40, 0x72, 0x03,
    ];

    /// 256 MiB of zeros, Brotli-compressed into 203 bytes.
    const ZEROS_256_MIB: &[u8] = &[
        0xcf, 0xff, 0xff, 0x7f, 0x00, 0x24, 0x00, 0xe2, 0xb1, 0x40, 0x72, 0xef, 0xff, 0xf9, 0xff,
        0xff, 0x0f, 0x80, 0x04, 0x40, 0x1c, 0x16, 0x80, 0xee, 0xfd, 0x3f, 0xff, 0xff, 0xff, 0x01,
        0x90, 0x00, 0x88, 0xc3, 0x02, 0xd0, 0xbd, 0xff, 0xe7, 0xff, 0xff, 0x3f, 0x00, 0x12, 0x00,
        0x71, 0x58, 0x00, 0xba, 0xf7, 0xff, 0xfc, 0xff, 0xff, 0x07, 0x40, 0x02, 0x20, 0x0e, 0x0b,
        0x40, 0xf7, 0xfe, 0x9f, 0xff, 0xff, 0xff, 0x00, 0x48, 0x00, 0xc4, 0x61, 0x01, 0xe8, 0xde,
        0xff, 0xf3, 0xff, 0xff, 0x1f, 0x00, 0x09, 0x80, 0x38, 0x2c, 0x00, 0xdd, 0xfb, 0x7f, 0xfe,
        0xff, 0xff, 0x03, 0x20, 0x01, 0x10, 0x87, 0x05, 0xa0, 0x7b, 0xff, 0xcf, 0xff, 0xff, 0x7f,
        0x00, 0x24, 0x00, 0xe2, 0xb0, 0x00, 0x74, 0xef, 0xff, 0xf9, 0xff, 0xff, 0x0f, 0x80, 0x04,
        0x40, 0x1c, 0x16, 0x80, 0xee, 0xfd, 0x3f, 0xff, 0xff, 0xff, 0x01, 0x90, 0x00, 0x88, 0xc3,
        0x02, 0xd0, 0xbd, 0xff, 0xe7, 0xff, 0xff, 0x3f, 0x00, 0x12, 0x00, 0x71, 0x58, 0x00, 0xba,
        0xf7, 0xff, 0xfc, 0xff, 0xff, 0x07, 0x40, 0x02, 0x20, 0x0e, 0x0b, 0x40, 0xf7, 0xfe, 0x9f,
        0xff, 0xff, 0xff, 0x00, 0x48, 0x00, 0xc4, 0x61, 0x01, 0xe8, 0xde, 0xff, 0xf3, 0xff, 0xff,
        0x1f, 0x00, 0x09, 0x80, 0x38, 0x2c, 0x00, 0xdd, 0xfb, 0x7f, 0xfe, 0xff, 0xff, 0x03, 0x20,
        0x01, 0x10, 0x87, 0x05, 0xa0, 0x7b, 0xff, 0x3f,
    ];

    /// A WOFF2 file with one `name` table that declares `orig_length`
    /// bytes, stored as the Brotli stream `data`, followed by `private`
    /// bytes of private data.
    fn woff2(orig_length: u32, data: &[u8], private: usize) -> Vec<u8> {
        let mut dir = vec![63]; // tag follows, null transform
        dir.extend_from_slice(b"name");
        // UIntBase128: seven bits a byte, most significant first.
        let mut n = orig_length;
        let mut digits = vec![(n & 0x7f) as u8];
        while n >= 0x80 {
            n >>= 7;
            digits.push(0x80 | (n & 0x7f) as u8);
        }
        dir.extend(digits.iter().rev());
        let data_end = (48 + dir.len() + data.len()).next_multiple_of(4);
        let len = data_end + private;
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(b"wOF2\0\x01\0\0");
        out.extend_from_slice(&(len as u32).to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes()); // numTables
        out.extend_from_slice(&[0; 2]); // reserved
        let sfnt_size = (u64::from(orig_length) + 28)
            .next_multiple_of(4)
            .min(u32::MAX.into()) as u32;
        out.extend_from_slice(&sfnt_size.to_be_bytes()); // totalSfntSize
        out.extend_from_slice(&(data.len() as u32).to_be_bytes()); // totalCompressedSize
        out.extend_from_slice(&[0, 1, 0, 0]); // version 1.0
        out.extend_from_slice(&[0; 12]); // no metadata
        let (offset, length) = if private > 0 {
            (data_end, private)
        } else {
            (0, 0)
        };
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(length as u32).to_be_bytes());
        out.extend_from_slice(&dir);
        out.extend_from_slice(data);
        out.resize(len, 0);
        out
    }

    #[test]
    fn woff2_bombs_fail_before_they_expand() {
        // The file is well formed: the same table with a stream of its
        // size decodes.
        let ordinary = woff2(1024, ZEROS_1_KIB, 0);
        // A 4 GiB table in a 300-byte file: rejected by its compression
        // ratio before anything is allocated or decompressed.
        let claims = woff2(u32::MAX, ZEROS_256_MIB, 0);
        assert!(claims.len() < 300);
        // A plausible ratio, padded with private data, but more than the
        // 128 MiB limit: rejected before decompressing.
        let padded = woff2(200 << 20, ZEROS_256_MIB, 3 << 20);
        // A stream that expands to 256 MiB in a table that declares 16 KiB:
        // decoded into a 16 KiB buffer, which it overflows.
        let expands = woff2(16 << 10, ZEROS_256_MIB, 0);
        // Decoding 256 MiB would take seconds; each bomb fails as fast as
        // the 1 KiB table decodes.
        for (what, bomb) in [
            ("claims", &claims),
            ("padded", &padded),
            ("expands", &expands),
        ] {
            crate::test_timing::assert_as_fast(
                what,
                || {
                    let t = std::time::Instant::now();
                    assert!(decode(&ordinary).unwrap().is_some());
                    t.elapsed()
                },
                || {
                    let t = std::time::Instant::now();
                    assert_eq!(decode(bomb).unwrap_err(), "not a valid WOFF2 font");
                    t.elapsed()
                },
            );
        }
        // Registration reports each as a font error.
        for bomb in [claims, padded, expands] {
            let err = super::super::FontStore::new()
                .register("bomb", bomb)
                .unwrap_err();
            assert_eq!(err.code(), "FONT_ERROR", "{err}");
        }
    }
}
