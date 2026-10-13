//! CFF helpers for embedding CFF-outline OpenType fonts as
//! `CIDFontType0C`.
//!
//! `subsetter` turns every CFF font into a CID-keyed one whose charset is
//! the identity (CID = new glyph id). Content streams use the original
//! glyph ids as CIDs, so a subset gets a new format 0 charset
//! that maps each new glyph id back to its original id. The Top DICT
//! writes the charset offset as a fixed 5-byte integer, so it is patched
//! in place and the charset is appended at the end.

/// The `CFF ` table of an OpenType font.
fn cff_table(otf: &[u8]) -> Option<&[u8]> {
    let u16_at = |p: usize| Some(u16::from_be_bytes(otf.get(p..p + 2)?.try_into().ok()?));
    let u32_at = |p: usize| Some(u32::from_be_bytes(otf.get(p..p + 4)?.try_into().ok()?));
    let n = usize::from(u16_at(4)?);
    (0..n).find_map(|i| {
        let rec = 12 + 16 * i;
        if otf.get(rec..rec + 4)? != b"CFF " {
            return None;
        }
        let off = u32_at(rec + 8)? as usize;
        let len = u32_at(rec + 12)? as usize;
        otf.get(off..off.checked_add(len)?)
    })
}

/// Largest real DICT operand `subsetter` gets to write back. subsetter
/// 0.2.6 prints a real with `{}` and panics on anything but digits, `.`
/// and `-` (`RealNumber::write` in `cff/number.rs`), so an operand that
/// overflows `f32` (`inf`) panics it; it also scales a Font DICT's
/// `FontMatrix` by 1000. Real fonts use values far below this. Check
/// `RealNumber::write` again when upgrading subsetter.
const MAX_REAL: f32 = 1e30;

/// Whether `subsetter` can write back every real operand of the DICTs
/// it rewrites: the Top DICT, the Font DICTs and their Private DICTs (see
/// [`MAX_REAL`]). `None` if they cannot be read.
pub(super) fn reals_fit(otf: &[u8]) -> Option<bool> {
    let cff = cff_table(otf)?;
    let header = usize::from(*cff.get(2)?);
    let (_, names_end) = index(cff, header)?;
    let (top, _) = index(cff, names_end)?;
    let mut dicts = vec![dict(cff, top.first()?.clone())?];
    // FDArray (12 36): the Font DICTs of a CID-keyed font.
    if let Some(at) = operand(&dicts[0], 0x0c24, 0) {
        let (fonts, _) = index(cff, usize::try_from(at as i64).ok()?)?;
        for font in fonts {
            dicts.push(dict(cff, font)?);
        }
    }
    let mut privates = Vec::new();
    for d in &dicts {
        // Private (18): size and offset.
        if let (Some(size), Some(at)) = (operand(d, 18, 0), operand(d, 18, 1)) {
            let at = usize::try_from(at as i64).ok()?;
            let end = at.checked_add(usize::try_from(size as i64).ok()?)?;
            privates.push(dict(cff, at..end)?);
        }
    }
    let fits = |d: &Dict| {
        d.iter()
            .flat_map(|(_, operands)| operands)
            .all(|v| v.is_finite() && v.abs() <= MAX_REAL)
    };
    Some(dicts.iter().chain(&privates).all(fits))
}

/// A DICT: its operators (two-byte ones as `12 << 8 | b`), each with its
/// operands.
type Dict = Vec<(u16, Vec<f32>)>;

/// Operand `k` of the first `op` entry of `d`.
fn operand(d: &Dict, op: u16, k: usize) -> Option<f32> {
    d.iter().find(|(o, _)| *o == op)?.1.get(k).copied()
}

/// Reads the DICT at `range` of `cff`, reals as `subsetter` reads them.
fn dict(cff: &[u8], range: std::ops::Range<usize>) -> Option<Dict> {
    let data = cff.get(range)?;
    let mut out = Vec::new();
    let mut operands = Vec::new();
    let mut p = 0;
    let byte = |p: usize| data.get(p).copied();
    while p < data.len() {
        let b = byte(p)?;
        let (value, len) = match b {
            0..=21 => {
                let (op, len) = if b == 12 {
                    (0x0c00 | u16::from(byte(p + 1)?), 2)
                } else {
                    (u16::from(b), 1)
                };
                out.push((op, std::mem::take(&mut operands)));
                p += len;
                continue;
            }
            28 => (
                f32::from(i16::from_be_bytes([byte(p + 1)?, byte(p + 2)?])),
                3,
            ),
            29 => {
                let v = i32::from_be_bytes(data.get(p + 1..p + 5)?.try_into().ok()?);
                (v as f32, 5)
            }
            30 => real(data.get(p + 1..)?).map(|(v, len)| (v, len + 1))?,
            32..=246 => (f32::from(i16::from(b) - 139), 1),
            247..=250 => (
                f32::from((i16::from(b) - 247) * 256 + i16::from(byte(p + 1)?) + 108),
                2,
            ),
            251..=254 => (
                f32::from(-(i16::from(b) - 251) * 256 - i16::from(byte(p + 1)?) - 108),
                2,
            ),
            _ => return None,
        };
        operands.push(value);
        p += len;
    }
    Some(out)
}

/// A real operand's value and length after its 30 prefix: nibbles to a
/// string parsed as `f32`, as `subsetter` does.
fn real(data: &[u8]) -> Option<(f32, usize)> {
    let mut s = String::new();
    for (i, &b) in data.iter().enumerate() {
        for n in [b >> 4, b & 0xf] {
            match n {
                0..=9 => s.push(char::from(b'0' + n)),
                10 => s.push('.'),
                11 => s.push('E'),
                12 => s.push_str("E-"),
                14 => s.push('-'),
                15 => return Some((s.parse().ok()?, i + 1)),
                _ => return None,
            }
        }
    }
    None
}

/// The whole font as a CID-keyed CFF with CID = glyph id: `subsetter`
/// keeping every glyph in order. `None` if the font cannot be converted.
pub(super) fn full_program(otf: &[u8]) -> Option<Vec<u8>> {
    if reals_fit(otf) != Some(true) {
        return None;
    }
    let n = ttf_parser::Face::parse(otf, 0).ok()?.number_of_glyphs();
    let glyphs: Vec<u16> = (0..n).collect();
    let remapper = subsetter::GlyphRemapper::new_from_glyphs_sorted(&glyphs);
    let sub = subsetter::subset(otf, 0, &remapper).ok()?;
    let cff = cff_table(&sub)?.to_vec();
    // The layout the subset path patches must be there too.
    charset_operand(&cff)?;
    Some(cff)
}

/// A subset of `glyphs` (sorted) whose CIDs are the original glyph ids.
pub(super) fn subset_program(otf: &[u8], glyphs: &[u16]) -> Option<Vec<u8>> {
    if reals_fit(otf) != Some(true) {
        return None;
    }
    let remapper = subsetter::GlyphRemapper::new_from_glyphs_sorted(glyphs);
    let sub = subsetter::subset(otf, 0, &remapper).ok()?;
    let mut cff = cff_table(&sub)?.to_vec();
    let cids: Vec<u16> = remapper.remapped_gids().skip(1).collect();
    set_charset(&mut cff, &cids)?;
    Some(cff)
}

/// Skips a CFF INDEX at `pos`; returns the range of its data and the end.
fn index(cff: &[u8], pos: usize) -> Option<(Vec<std::ops::Range<usize>>, usize)> {
    let count = usize::from(u16::from_be_bytes(cff.get(pos..pos + 2)?.try_into().ok()?));
    if count == 0 {
        return Some((Vec::new(), pos + 2));
    }
    let off_size = usize::from(*cff.get(pos + 2)?);
    if !(1..=4).contains(&off_size) {
        return None;
    }
    let offsets_at = pos + 3;
    let offset = |i: usize| -> Option<usize> {
        let b = cff.get(offsets_at + i * off_size..offsets_at + (i + 1) * off_size)?;
        Some(b.iter().fold(0usize, |v, &x| (v << 8) | usize::from(x)))
    };
    let data = offsets_at + (count + 1) * off_size - 1;
    let mut items = Vec::with_capacity(count);
    for i in 0..count {
        let (a, b) = (offset(i)?, offset(i + 1)?);
        if a == 0 || b < a {
            return None;
        }
        items.push(data + a..data + b);
    }
    let end = data + offset(count)?;
    (end <= cff.len()).then_some((items, end))
}

/// Position of the 4 value bytes of the Top DICT `charset` operand, when
/// it is written as a 5-byte integer.
fn charset_operand(cff: &[u8]) -> Option<usize> {
    let header = usize::from(*cff.get(2)?);
    let (_, names_end) = index(cff, header)?;
    let (top, _) = index(cff, names_end)?;
    let dict = top.first()?.clone();
    let mut p = dict.start;
    let mut last_operand = None;
    while p < dict.end {
        let b = *cff.get(p)?;
        match b {
            12 => {
                p += 2;
                last_operand = None;
            }
            15 => {
                return last_operand
                    .filter(|&o| cff.get(o) == Some(&29))
                    .map(|o| o + 1)
            }
            0..=21 => {
                p += 1;
                last_operand = None;
            }
            28 => {
                last_operand = Some(p);
                p += 3;
            }
            29 => {
                last_operand = Some(p);
                p += 5;
            }
            30 => {
                last_operand = Some(p);
                p += 1;
                // Nibbles until an 0xf terminator.
                loop {
                    let x = *cff.get(p)?;
                    p += 1;
                    if x >> 4 == 0xf || x & 0xf == 0xf {
                        break;
                    }
                }
            }
            32..=246 => {
                last_operand = Some(p);
                p += 1;
            }
            247..=254 => {
                last_operand = Some(p);
                p += 2;
            }
            _ => return None,
        }
    }
    None
}

/// Replaces the charset of a CID-keyed CFF: glyph `i + 1` gets CID
/// `cids[i]` (glyph 0 is always CID 0). `None` if the font does not have
/// the layout `subsetter` writes.
fn set_charset(cff: &mut Vec<u8>, cids: &[u16]) -> Option<()> {
    let at = charset_operand(cff)?;
    let offset = i32::try_from(cff.len()).ok()?;
    cff.get_mut(at..at + 4)?
        .copy_from_slice(&offset.to_be_bytes());
    cff.reserve(1 + 2 * cids.len());
    cff.push(0);
    for c in cids {
        cff.extend_from_slice(&c.to_be_bytes());
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Header, Name INDEX ("A"), Top DICT INDEX with `charset` as a 5-byte
    /// integer after a real and a two-byte operator.
    fn tiny() -> Vec<u8> {
        let mut dict = vec![30, 0x1f, 12, 30]; // real 1, then op 12 30
        dict.extend([29, 0, 0, 0, 0, 15]); // charset 0
        dict.extend([139, 17]); // CharStrings 0
        let mut v = vec![1, 0, 4, 1];
        v.extend([0, 1, 1, 1, 2, b'A']);
        v.extend([0, 1, 1, 1, dict.len() as u8 + 1]);
        v.extend(dict);
        v
    }

    #[test]
    fn reads_dict_numbers_as_subsetter_does() {
        assert_eq!(real(&[0x1f, 0]), Some((1.0, 1)));
        assert_eq!(real(&[0x0a, 0x5f]), Some((0.5, 2)));
        assert_eq!(real(&[0xe2, 0xc3, 0xff]), Some((-2e-3, 3)));
        // 1E999 overflows f32, as in subsetter.
        assert_eq!(real(&[0x1b, 0x99, 0x9f]), Some((f32::INFINITY, 3)));
        assert_eq!(real(&[0x1d, 0xff]), None);
        assert_eq!(real(&[0x11]), None);
        let c = tiny();
        let top = dict(&c, 15..c.len()).unwrap();
        assert_eq!(
            top,
            vec![(0x0c1e, vec![1.0]), (15, vec![0.0]), (17, vec![0.0])]
        );
        assert_eq!(
            dict(&[28, 0x80, 0, 250, 0xff, 254, 0xff, 1], 0..8),
            Some(vec![(1, vec![-32768.0, 1131.0, -1131.0])])
        );
        assert_eq!(dict(&[255, 1], 0..2), None);
    }

    #[test]
    fn patches_charset() {
        let mut c = tiny();
        let len = c.len();
        set_charset(&mut c, &[7, 300]).unwrap();
        let at = charset_operand(&c).unwrap();
        let off = i32::from_be_bytes(c[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!(off, len);
        assert_eq!(&c[off..], &[0, 0, 7, 1, 44]);
    }

    #[test]
    fn rejects_short_offsets() {
        let mut c = tiny();
        let p = c.iter().position(|&b| b == 29).unwrap();
        c.splice(p..p + 5, [139]); // charset as a one-byte number
        c[14] -= 4; // shrink the Top DICT INDEX's end offset
        assert_eq!(set_charset(&mut c, &[1]), None);
        assert_eq!(cff_table(b"\0\x01\0\0\0\x01"), None);
    }
}
