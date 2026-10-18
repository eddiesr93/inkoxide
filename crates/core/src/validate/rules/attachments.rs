//! The checks of attachments: the files a document declares, the icons
//! that open them, and the XMP schemas PDF/A metadata declares for them.

use std::collections::BTreeMap;

use super::{Checker, MAX_NOTE};
use crate::attachments::{
    FileSource, MAX_ATTACHMENTS, MAX_ATTACHMENTS_TOTAL, MAX_ATTACHMENT_BYTES, MAX_FILE_NAME,
    MAX_MIME_TYPE,
};
use crate::date::parse_rfc3339;
use crate::error::{ValidationCode as Code, ValidationError};
use crate::schema::{Attachment, AttachmentDecl, XmpSchema};
use crate::PdfALevel;

/// Most schemas `meta.xmpExtensions` declares.
pub(crate) const MAX_XMP_SCHEMAS: usize = 16;
/// Most properties of one schema.
pub(crate) const MAX_XMP_PROPERTIES: usize = 64;
/// Longest namespace URI, schema description or property name, in
/// characters.
pub(crate) const MAX_XMP_NAME: usize = 512;
/// Longest property value, in characters: as long as a note's text.
pub(crate) const MAX_XMP_VALUE: usize = MAX_NOTE;

/// Prefixes the XMP packet already binds, or XML reserves.
const RESERVED_PREFIXES: &[&str] = &[
    "pdfaid",
    "dc",
    "xmp",
    "pdf",
    "rdf",
    "x",
    "xml",
    "xmlns",
    "pdfaExtension",
    "pdfaSchema",
    "pdfaProperty",
    "pdfuaid",
];

impl Checker<'_> {
    /// The files of `attachments`: at most [`MAX_ATTACHMENTS`], each with a
    /// name used once, bytes that exist and fit, a MIME type and a date
    /// that parse; the level of PDF/A must allow them.
    pub(super) fn attachments(&mut self, decls: &[AttachmentDecl]) {
        if decls.len() > MAX_ATTACHMENTS {
            self.err(
                Code::OutOfRange,
                Some("attachments"),
                format!("at most {MAX_ATTACHMENTS} attachments"),
            );
            return;
        }
        let mut names = BTreeMap::new();
        let mut total = 0usize;
        for (i, d) in decls.iter().enumerate() {
            self.at("attachments", |c| {
                c.at_index(i, |c| {
                    c.file_name(&d.name);
                    if let Some(prev) = names.insert(d.name.as_str(), i) {
                        c.err(
                            Code::DuplicateId,
                            Some("name"),
                            format!("attachment `{}` is already declared at attachments[{prev}]", d.name),
                        );
                    }
                    total = total.saturating_add(c.file_bytes(d));
                    if let Some(m) = &d.mime_type {
                        if !is_mime_type(m) {
                            c.err(
                                Code::InvalidValue,
                                Some("mimeType"),
                                format!(
                                    "a MIME type is `type/subtype` of letters, digits and `!#$&-^_.+`, at most {MAX_MIME_TYPE} bytes"
                                ),
                            );
                        }
                    }
                    if d.description.as_ref().is_some_and(|t| t.chars().count() > MAX_NOTE) {
                        c.err(
                            Code::InvalidValue,
                            Some("description"),
                            format!("a description is at most {MAX_NOTE} characters"),
                        );
                    }
                    if let Some(m) = &d.modified {
                        if parse_rfc3339(m).is_none() {
                            c.err(
                                Code::InvalidValue,
                                Some("modified"),
                                format!("`{m}` is not an RFC 3339 timestamp"),
                            );
                        }
                    }
                });
            });
        }
        if total > MAX_ATTACHMENTS_TOTAL {
            self.err(
                Code::AttachmentTooLarge,
                Some("attachments"),
                format!(
                    "the attachments are {} MiB together; the limit is {} MiB",
                    total >> 20,
                    MAX_ATTACHMENTS_TOTAL >> 20
                ),
            );
        }
        let refused = match self.profile.level() {
            Some(PdfALevel::A1b) => Some("PDF/A-1b does not allow attachments"),
            Some(PdfALevel::A2b) => Some(
                "PDF/A-2b allows only PDF/A files as attachments, which Inkoxide cannot check; render PDF/A-3b",
            ),
            Some(PdfALevel::A2a) => Some(
                "PDF/A-2a allows only PDF/A files as attachments, which Inkoxide cannot check; render PDF/A-3a",
            ),
            _ => None,
        };
        if let (Some(why), false) = (refused, decls.is_empty()) {
            self.conflict.get_or_insert(ValidationError {
                code: Code::PdfaConflict,
                path: "attachments[0]".into(),
                message: why.into(),
            });
        }
    }

    /// A file name: 1 to [`MAX_FILE_NAME`] characters, no path separator
    /// and no control character.
    fn file_name(&mut self, name: &str) {
        let n = name.chars().count();
        if !(1..=MAX_FILE_NAME).contains(&n)
            || name
                .chars()
                .any(|ch| ch == '/' || ch == '\\' || ch.is_control())
        {
            self.err(
                Code::InvalidValue,
                Some("name"),
                format!(
                    "a file name is 1 to {MAX_FILE_NAME} characters, without `/`, `\\` or control characters"
                ),
            );
        }
    }

    /// The size of file `d`'s bytes, checked to exist and to fit; 0 when
    /// unknown.
    fn file_bytes(&mut self, d: &AttachmentDecl) -> usize {
        let (field, len) = match &d.source {
            FileSource::Ref(id) => {
                if !self.id(Some("src"), id) {
                    return 0;
                }
                let Some(store) = self.attachments else {
                    return 0;
                };
                let Some(len) = store.len_of(id) else {
                    self.err(
                        Code::AttachmentNotRegistered,
                        Some("src"),
                        format!("attachment `{id}` was not provided in the render options"),
                    );
                    return 0;
                };
                ("src", usize::try_from(len).unwrap_or(usize::MAX))
            }
            FileSource::Inline(inline) => {
                // Too large is told before decoding.
                if inline.max_len() <= MAX_ATTACHMENT_BYTES + 3 {
                    match inline.bytes() {
                        Ok(b) => ("data", b.len()),
                        Err(e) => {
                            self.err(Code::InvalidValue, Some("data"), e);
                            return 0;
                        }
                    }
                } else {
                    ("data", inline.max_len())
                }
            }
        };
        if len > MAX_ATTACHMENT_BYTES {
            self.err(
                Code::AttachmentTooLarge,
                Some(field),
                format!(
                    "an attachment is at most {} MiB, and this one is {} MiB",
                    MAX_ATTACHMENT_BYTES >> 20,
                    len >> 20
                ),
            );
        }
        len
    }

    /// An attachment icon: it names a declared file, and its text and
    /// colour are a note's.
    pub(super) fn attachment(&mut self, a: &Attachment) {
        if !self.doc.attachments.iter().any(|d| d.name == a.file) {
            self.err(
                Code::UnknownAttachment,
                Some("file"),
                format!("attachment `{}` is not declared in `attachments`", a.file),
            );
        }
        if a.contents
            .as_ref()
            .is_some_and(|t| !(1..=MAX_NOTE).contains(&t.chars().count()))
        {
            self.err(
                Code::InvalidValue,
                Some("contents"),
                format!("an attachment's contents are 1 to {MAX_NOTE} characters"),
            );
        }
        if let Some(c) = &a.color {
            self.color_as(Some("color"), c, super::Alpha::Ignored);
        }
    }

    /// `meta.xmpExtensions`: at most [`MAX_XMP_SCHEMAS`] schemas, each with
    /// a namespace, a prefix of its own and at most [`MAX_XMP_PROPERTIES`]
    /// properties with XML names. They go in the PDF/A metadata, so they
    /// need PDF/A.
    pub(super) fn xmp_extensions(&mut self, schemas: &[XmpSchema]) {
        if schemas.len() > MAX_XMP_SCHEMAS {
            self.err(
                Code::OutOfRange,
                None,
                format!("at most {MAX_XMP_SCHEMAS} XMP schemas"),
            );
            return;
        }
        let mut prefixes = BTreeMap::new();
        for (i, schema) in schemas.iter().enumerate() {
            self.at_index(i, |c| {
                let long = |s: &str| !(1..=MAX_XMP_NAME).contains(&s.chars().count());
                if long(&schema.schema) {
                    c.err(
                        Code::InvalidValue,
                        Some("schema"),
                        format!("a schema description is 1 to {MAX_XMP_NAME} characters"),
                    );
                }
                if long(&schema.namespace) || schema.namespace.chars().any(char::is_whitespace) {
                    c.err(
                        Code::InvalidValue,
                        Some("namespace"),
                        format!("a namespace is a URI of 1 to {MAX_XMP_NAME} characters"),
                    );
                }
                if !is_xml_name(&schema.prefix) || RESERVED_PREFIXES.contains(&schema.prefix.as_str()) {
                    c.err(
                        Code::InvalidValue,
                        Some("prefix"),
                        "a prefix is an XML name the metadata does not use already (not `pdfaid`, `dc`, `xmp`, `pdf`, `rdf`, …)",
                    );
                } else if let Some(prev) = prefixes.insert(schema.prefix.as_str(), i) {
                    c.err(
                        Code::DuplicateId,
                        Some("prefix"),
                        format!("prefix `{}` is already declared at xmpExtensions[{prev}]", schema.prefix),
                    );
                }
                if schema.properties.len() > MAX_XMP_PROPERTIES {
                    c.err(
                        Code::OutOfRange,
                        Some("properties"),
                        format!("a schema has at most {MAX_XMP_PROPERTIES} properties"),
                    );
                    return;
                }
                for (j, p) in schema.properties.iter().enumerate() {
                    c.at("properties", |c| {
                        c.at_index(j, |c| {
                            if !is_xml_name(&p.name) || p.name.chars().count() > MAX_XMP_NAME {
                                c.err(Code::InvalidValue, Some("name"), "a property name is an XML name");
                            }
                            if p.value.chars().count() > MAX_XMP_VALUE {
                                c.err(
                                    Code::InvalidValue,
                                    Some("value"),
                                    format!("a value is at most {MAX_XMP_VALUE} characters"),
                                );
                            }
                            if long(&p.description) {
                                c.err(
                                    Code::InvalidValue,
                                    Some("description"),
                                    format!("a description is 1 to {MAX_XMP_NAME} characters"),
                                );
                            }
                        });
                    });
                }
            });
        }
        if self.rendering && !self.profile.is_pdf_a() && !schemas.is_empty() {
            self.err(
                Code::InvalidValue,
                None,
                "XMP extensions go in the PDF/A metadata: render PDF/A",
            );
        }
    }
}

/// `type/subtype` in RFC 6838's restricted-name characters, at most
/// [`MAX_MIME_TYPE`] bytes.
fn is_mime_type(m: &str) -> bool {
    let name = |s: &str| {
        (1..=63).contains(&s.len())
            && s.starts_with(|c: char| c.is_ascii_alphanumeric())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&b))
    };
    m.len() <= MAX_MIME_TYPE && m.split_once('/').is_some_and(|(t, s)| name(t) && name(s))
}

/// An XML name without a colon (`NCName`), in ASCII.
fn is_xml_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}
