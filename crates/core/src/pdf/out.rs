//! The one place the writer puts strings and streams into the file.
//!
//! An encrypted file has every string and stream outside the `Encrypt`
//! dictionary encrypted, but for the cross-reference stream and the
//! strings inside object streams (encrypted whole, `objstm.rs`), so none
//! may reach the file except through here:
//! `clippy.toml` forbids `TextStr`, pdf-writer's other writers that make
//! strings of their own (`TextStrWithLang`, `Date`, …) and its stream
//! writers elsewhere in the crate. Strings inside content streams are left alone,
//! since the whole stream is encrypted.
//!
//! Unencrypted, the bytes are exactly what `TextStr` and the stream
//! writers produce, so plain output does not depend on this module.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::borrow::Cow;

use pdf_writer::writers::{Cmap, FormXObject, IccProfile, ImageXObject, Metadata, TilingPattern};
use pdf_writer::{Chunk, Ref, Stream};

use super::crypt::Crypt;

/// The bytes of text string `s` as `TextStr` writes them: printable ASCII
/// as it is, anything else as a byte order mark and UTF-16BE. Inside a
/// content stream, where nothing is encrypted string by string, write
/// them as a `Str`; elsewhere use [`Out::text`].
pub(crate) fn text_bytes(s: &str) -> Vec<u8> {
    if s.bytes().all(|b| matches!(b, 32..=126)) {
        s.as_bytes().to_vec()
    } else {
        let mut utf16 = vec![0xFE, 0xFF];
        utf16.extend(s.encode_utf16().flat_map(u16::to_be_bytes));
        utf16
    }
}

/// Most bytes text string `s` takes in the file once written through
/// [`Out::text`], delimiters included: printable ASCII at most doubled by
/// escapes, anything else as UTF-16BE in hex, four bytes a code unit, and
/// encrypted strings in hex with up to 32 bytes of padding and IV first.
pub(crate) fn written_text_len(s: &str) -> usize {
    let bytes = if s.bytes().all(|b| matches!(b, 32..=126)) {
        s.len()
    } else {
        2 + 2 * s.encode_utf16().count()
    };
    written_len(bytes)
}

/// [`written_text_len`] of the longest text of `chars` characters: two
/// UTF-16 code units each.
pub(crate) const fn most_written_text_len(chars: usize) -> usize {
    written_len(2 + 4 * chars)
}

/// Most bytes a string of `bytes` bytes takes in the file: at most
/// doubled by escapes or hex, and encrypted with up to 32 bytes of padding
/// and IV first (`Crypt::seal`).
pub(super) const fn written_len(bytes: usize) -> usize {
    2 + 2 * (bytes + 32)
}

/// Strings and streams on their way into the file, encrypted when the
/// file is.
pub(crate) struct Out {
    crypt: Option<Crypt>,
}

impl Out {
    /// Writes strings and streams as they are.
    pub fn plain() -> Self {
        Self { crypt: None }
    }

    /// Encrypts every string and stream with `crypt`.
    pub fn encrypted(crypt: Crypt) -> Self {
        Self { crypt: Some(crypt) }
    }

    /// Whether strings and streams are encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.crypt.is_some()
    }

    /// The tag of a font subset as written: `tag` itself, or when
    /// encrypted one keyed by the file's keys, since font names are not
    /// encrypted. An empty tag (a whole font) stays empty.
    pub fn subset_tag(&self, tag: &str, font: &str) -> String {
        match &self.crypt {
            Some(c) if !tag.is_empty() => c.subset_tag(tag, font),
            _ => tag.to_string(),
        }
    }

    /// A text string, to write as a `Str`: see [`text_bytes`].
    pub fn text(&mut self, s: &str) -> Vec<u8> {
        self.bytes(&text_bytes(s))
    }

    /// A byte string, to write as a `Str`.
    pub fn bytes(&mut self, b: &[u8]) -> Vec<u8> {
        match &mut self.crypt {
            Some(c) => c.string(b),
            None => b.to_vec(),
        }
    }

    /// The data of stream object `r`.
    fn data<'d>(&self, r: Ref, data: &'d [u8]) -> Cow<'d, [u8]> {
        match &self.crypt {
            Some(c) => Cow::Owned(c.stream(r, data)),
            None => Cow::Borrowed(data),
        }
    }

    /// Writes stream `r` holding `data`; `f` fills in its dictionary.
    pub fn stream<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut Stream<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.stream(r, &data);
        f(&mut w)
    }

    /// Writes image `XObject` `r`; `f` fills in its dictionary.
    pub fn image_xobject<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut ImageXObject<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.image_xobject(r, &data);
        f(&mut w)
    }

    /// Writes form `XObject` `r`; `f` fills in its dictionary.
    pub fn form_xobject<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut FormXObject<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.form_xobject(r, &data);
        f(&mut w)
    }

    /// Writes tiling pattern `r`; `f` fills in its dictionary.
    pub fn tiling_pattern<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut TilingPattern<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.tiling_pattern(r, &data);
        f(&mut w)
    }

    /// Writes `CMap` stream `r`; `f` fills in its dictionary.
    pub fn cmap<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut Cmap<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.cmap(r, &data);
        f(&mut w)
    }

    /// Writes metadata stream `r`; `f` fills in its dictionary.
    pub fn metadata<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut Metadata<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.metadata(r, &data);
        f(&mut w)
    }

    /// Writes ICC profile stream `r`; `f` fills in its dictionary.
    pub fn icc_profile<R>(
        &mut self,
        pdf: &mut Chunk,
        r: Ref,
        data: &[u8],
        f: impl FnOnce(&mut IccProfile<'_>) -> R,
    ) -> R {
        let data = self.data(r, data);
        let mut w = pdf.icc_profile(r, &data);
        f(&mut w)
    }
}

#[cfg(test)]
mod tests {
    use pdf_writer::{Chunk, Ref, Str, TextStr};

    use super::{written_text_len, Out};

    #[test]
    fn written_text_len_bounds_what_text_strings_take() {
        let written = |s: &str| {
            let mut c = Chunk::new();
            c.indirect(Ref::new(1)).primitive(TextStr(s));
            let mut empty = Chunk::new();
            empty.indirect(Ref::new(1)).primitive(TextStr(""));
            c.as_bytes().len() - empty.as_bytes().len() + 2
        };
        for s in [
            String::new(),
            "x".repeat(100),
            ")".repeat(100),
            format!("{}é", "x".repeat(100)),
            "\n\n".repeat(50),
            "😀".repeat(100),
        ] {
            let n = written(&s);
            assert!(n <= written_text_len(&s), "{s}: {n}");
            // Close, apart from the room for encryption.
            assert!(written_text_len(&s) <= 2 * n + 66, "{s}: {n}");
        }
    }

    #[test]
    fn plain_text_is_written_as_text_str_writes_it() {
        for s in [
            "",
            "Plain ASCII (with parentheses) and \\ backslash",
            "Café № 5",
            "tab\tand\nnewline",
            "emoji 😀",
            "(unbalanced",
        ] {
            let mut a = Chunk::new();
            a.indirect(Ref::new(1)).primitive(TextStr(s));
            let mut b = Chunk::new();
            b.indirect(Ref::new(1))
                .primitive(Str(&Out::plain().text(s)));
            assert_eq!(a.as_bytes(), b.as_bytes(), "{s}");
        }
    }

    /// `clippy.toml` lists every pdf-writer 0.15 writer of streams, and of
    /// strings it makes itself, so none is used outside this module.
    #[test]
    fn clippy_forbids_the_other_writers_of_strings_and_streams() {
        let config = include_str!("../../clippy.toml");
        for path in [
            "pdf_writer::TextStr",
            "pdf_writer::TextStrWithLang",
            "pdf_writer::Date",
            "pdf_writer::Date::new",
            "pdf_writer::writers::ColorSpace::indexed",
            "pdf_writer::writers::SeparationInfo::device_colorant_str",
            "pdf_writer::writers::FontDescriptor::style",
            "pdf_writer::writers::MediaPermissions::temp_file",
            "pdf_writer::Chunk::stream",
            "pdf_writer::Chunk::image_xobject",
            "pdf_writer::Chunk::form_xobject",
            "pdf_writer::Chunk::tiling_pattern",
            "pdf_writer::Chunk::cmap",
            "pdf_writer::Chunk::metadata",
            "pdf_writer::Chunk::icc_profile",
            "pdf_writer::Chunk::embedded_file",
            "pdf_writer::Chunk::stream_shading",
            "pdf_writer::Chunk::sampled_function",
            "pdf_writer::Chunk::post_script_function",
        ] {
            assert!(
                config.contains(&format!("path = \"{path}\"")),
                "{path} is not in clippy.toml"
            );
        }
    }

    #[test]
    fn plain_streams_are_written_as_they_are() {
        let mut a = Chunk::new();
        a.stream(Ref::new(1), b"data");
        let mut b = Chunk::new();
        Out::plain().stream(&mut b, Ref::new(1), b"data", |_| {});
        assert_eq!(a.as_bytes(), b.as_bytes());
    }
}
