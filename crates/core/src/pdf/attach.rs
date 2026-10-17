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
