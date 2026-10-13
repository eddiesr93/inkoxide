//! The one place the writer puts strings and streams into the file.
//!
//! An encrypted file has every string and stream outside the `Encrypt`
//! dictionary encrypted, so none may reach the file except through here:
//! `clippy.toml` forbids `TextStr` and pdf-writer's stream writers
//! elsewhere in the crate. Strings inside content streams are left alone,
//! since the whole stream is encrypted.
//!
//! Unencrypted, the bytes are exactly what `TextStr` and the stream
//! writers produce, so plain output does not depend on this module.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::borrow::Cow;

use pdf_writer::writers::{Cmap, FormXObject, IccProfile, ImageXObject, Metadata, TilingPattern};
use pdf_writer::{Chunk, Ref, Stream};

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

/// Strings and streams on their way into the file, encrypted when the
/// file is.
pub(crate) struct Out {}

impl Out {
    /// Writes strings and streams as they are.
    pub fn plain() -> Self {
        Self {}
    }

    /// A text string, to write as a `Str`: see [`text_bytes`].
    pub fn text(&mut self, s: &str) -> Vec<u8> {
        self.bytes(&text_bytes(s))
    }

    /// A byte string, to write as a `Str`.
    #[expect(clippy::unused_self, reason = "encryption uses it next")]
    pub fn bytes(&mut self, b: &[u8]) -> Vec<u8> {
        b.to_vec()
    }

    /// The data of stream object `r`.
    #[expect(clippy::unused_self, reason = "encryption uses it next")]
    fn data<'d>(&mut self, _r: Ref, data: &'d [u8]) -> Cow<'d, [u8]> {
        Cow::Borrowed(data)
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

    use super::Out;

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

    #[test]
    fn plain_streams_are_written_as_they_are() {
        let mut a = Chunk::new();
        a.stream(Ref::new(1), b"data");
        let mut b = Chunk::new();
        Out::plain().stream(&mut b, Ref::new(1), b"data", |_| {});
        assert_eq!(a.as_bytes(), b.as_bytes());
    }
}
