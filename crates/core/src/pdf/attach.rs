//! Embedded files: one `EmbeddedFile` stream and one `Filespec` per
//! declared attachment, which the catalog's `/EmbeddedFiles` name tree
//! lists (and, under PDF/A-3, its `/AF` array), and which attachment icons
//! open.

use std::collections::BTreeMap;

use pdf_writer::{Filter, Finish, Name, Ref, Str};

use super::document::pdf_date;
use super::out::text_bytes;
use super::Writer;
use crate::attachments::PreparedFile;
use crate::schema::Relationship;

/// The file specifications written, by file name, with what an icon shows
/// of each.
#[derive(Default)]
pub(super) struct FileRefs<'a> {
    pub specs: Vec<Ref>,
    pub by_name: BTreeMap<&'a str, (Ref, &'a PreparedFile)>,
}

/// Writes every file: its stream, then its file specification, in
/// declaration order. Nothing is allocated without files, so other object
/// numbers stay as they were.
pub(super) fn write_files<'a>(w: &mut Writer, files: &'a [PreparedFile]) -> FileRefs<'a> {
    let mut refs = FileRefs::default();
    for f in files {
        let spec = w.alloc.next();
        let stream = w.alloc.next();
        let modified = f.modified.map(|t| w.out.bytes(pdf_date(t).as_bytes()));
        w.out.stream(&mut w.pdf, stream, &f.data, |s| {
            s.pair(Name(b"Type"), Name(b"EmbeddedFile"));
            s.pair(Name(b"Subtype"), Name(f.mime_type.as_bytes()));
            if f.compressed {
                s.filter(Filter::FlateDecode);
            }
            let mut p = s.insert(Name(b"Params")).dict();
            p.pair(Name(b"Size"), i32::try_from(f.size).unwrap_or(i32::MAX));
            if let Some(d) = &modified {
                p.pair(Name(b"ModDate"), Str(d));
            }
        });
        let ascii = w.out.bytes(ascii_name(&f.name).as_bytes());
        let unicode = w.out.text(&f.name);
        let description = f.description.as_deref().map(|d| w.out.text(d));
        let mut d = w.pdf.indirect(spec).dict();
        d.pair(Name(b"Type"), Name(b"Filespec"));
        d.pair(Name(b"F"), Str(&ascii));
        d.pair(Name(b"UF"), Str(&unicode));
        if let Some(desc) = &description {
            d.pair(Name(b"Desc"), Str(desc));
        }
        d.insert(Name(b"EF"))
            .dict()
            .pair(Name(b"F"), stream)
            .pair(Name(b"UF"), stream);
        d.pair(Name(b"AFRelationship"), Name(relationship(f.relationship)));
        d.finish();
        refs.specs.push(spec);
        refs.by_name.insert(f.name.as_str(), (spec, f));
    }
    refs
}

/// The entries of the `/EmbeddedFiles` name tree: each file's name as
/// written (through `encode`, which encrypts when the file is) and its
/// specification, in key order. Readers compare keys as bytes or as text,
/// and both orders agree when every key is ASCII, or every key UTF-16:
/// one non-ASCII name makes them all UTF-16.
pub(super) fn name_tree(
    files: &[PreparedFile],
    specs: &[Ref],
    mut encode: impl FnMut(&[u8]) -> Vec<u8>,
) -> Vec<(Vec<u8>, Ref)> {
    let ascii = files
        .iter()
        .all(|f| text_bytes(&f.name) == f.name.as_bytes());
    let mut keys: Vec<(Vec<u8>, Ref)> = files
        .iter()
        .zip(specs)
        .map(|(f, &r)| {
            let key = if ascii {
                f.name.as_bytes().to_vec()
            } else {
                let units = f.name.encode_utf16().flat_map(u16::to_be_bytes);
                [0xFE, 0xFF].into_iter().chain(units).collect()
            };
            (key, r)
        })
        .collect();
    keys.sort();
    keys.into_iter().map(|(key, r)| (encode(&key), r)).collect()
}

/// The `/F` name: the file name with each character outside printable
/// ASCII as `_` (`/UF` holds the name itself).
fn ascii_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn relationship(r: Relationship) -> &'static [u8] {
    match r {
        Relationship::Source => b"Source",
        Relationship::Data => b"Data",
        Relationship::Alternative => b"Alternative",
        Relationship::Supplement => b"Supplement",
        Relationship::Unspecified => b"Unspecified",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PreparedFile {
        PreparedFile {
            name: name.into(),
            mime_type: "text/plain".into(),
            description: None,
            relationship: Relationship::Unspecified,
            modified: None,
            data: Vec::new(),
            compressed: false,
            size: 0,
            hash: 0,
        }
    }

    fn utf16(s: &str) -> Vec<u8> {
        [0xFE, 0xFF]
            .into_iter()
            .chain(s.encode_utf16().flat_map(u16::to_be_bytes))
            .collect()
    }

    #[test]
    fn name_tree_keys_are_sorted_bytes_ascii_or_all_utf16() {
        let refs = [Ref::new(1), Ref::new(2), Ref::new(3)];
        let ascii = [file("b.xml"), file("a.pdf"), file("C.txt")];
        assert_eq!(
            name_tree(&ascii, &refs, <[u8]>::to_vec),
            [
                (b"C.txt".to_vec(), Ref::new(3)),
                (b"a.pdf".to_vec(), Ref::new(2)),
                (b"b.xml".to_vec(), Ref::new(1)),
            ]
        );
        // One name outside printable ASCII makes every key UTF-16.
        let mixed = [file("é.xml"), file("b.xml"), file("Z\t.txt")];
        assert_eq!(
            name_tree(&mixed, &refs, <[u8]>::to_vec),
            [
                (utf16("Z\t.txt"), Ref::new(3)),
                (utf16("b.xml"), Ref::new(2)),
                (utf16("é.xml"), Ref::new(1)),
            ]
        );
        // Keys are sorted as written, then encoded (encrypted).
        let tree = name_tree(&ascii, &refs, |k| k.iter().rev().copied().collect());
        assert_eq!(tree[0], (b"txt.C".to_vec(), Ref::new(3)));
    }

    #[test]
    fn ascii_names_replace_each_other_character() {
        assert_eq!(
            ascii_name("Rechnung März 2026.xml"),
            "Rechnung M_rz 2026.xml"
        );
        assert_eq!(ascii_name("a\tb\u{1F600}.txt"), "a_b_.txt");
        assert_eq!(ascii_name("~!.csv"), "~!.csv");
    }
}
