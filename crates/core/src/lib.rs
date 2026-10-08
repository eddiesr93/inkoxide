//! Inkoxide: a native PDF layout and rendering engine.
//!
//! ```text
//! render(document_tree, options) -> pdf_bytes
//! ```
//!
//! The input is a fully resolved document tree (see
//! `schema/document.v1.json`); the engine knows nothing about HTML, CSS or
//! React. This crate does no I/O: bytes in, bytes out.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod date;
mod error;
mod fonts;
mod images;
mod layout;
mod lru;
mod paginate;
mod paint;
mod pdf;
mod pipeline;
mod pool;
mod schema;
mod text;
mod validate;

pub use error::{InkoxideError, ValidationCode, ValidationError};
pub use fonts::FontStore;
pub use images::ImageStore;
pub use schema::SCHEMA_VERSION;

use fonts::FontSet;
use pdf::{DocumentOut, PdfSink, PdfWriterSink};
use text::{RustybuzzShaper, TextCache, UnicodeMap};

/// Engine version (the crate version).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serialisation of the document tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputFormat {
    /// JSON text (UTF-8).
    #[default]
    Json,
    /// `MessagePack` with the same structure as the JSON (maps keyed by the
    /// JSON field names).
    MessagePack,
}

/// Output settings for [`render`]; the document describes content only.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Flate-compress content streams (default `true`).
    pub compress: bool,
    /// Flate level 0–9 used when `compress` is set (default 6).
    pub compression_level: u32,
    /// RFC 3339 timestamp; overrides `meta.creationDate`. With neither set
    /// no date is written, so output is byte-for-byte deterministic.
    pub creation_date: Option<String>,
    /// Worker threads for page painting and compression; `None` uses the
    /// shared pool (all cores), `Some(1)` renders on the calling thread.
    pub threads: Option<usize>,
    /// Embed only the glyphs a document uses (default `true`). Off embeds
    /// whole font programs, as before M7.
    pub subset_fonts: bool,
    /// Write PDF/A-2b (default `false`): XMP metadata, an sRGB output
    /// intent and a file identifier; CMYK JPEGs are converted to RGB and
    /// `.notdef` glyphs are left out (ADR-039).
    pub pdf_a: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            compress: true,
            compression_level: 6,
            creation_date: None,
            threads: None,
            subset_fonts: true,
            pdf_a: false,
        }
    }
}

/// Validates a document against the schema and the registered resources.
/// Returns every problem found (capped at 100); empty means renderable.
#[must_use]
pub fn validate(
    doc: &[u8],
    format: InputFormat,
    fonts: &FontStore,
    images: &ImageStore,
) -> Vec<ValidationError> {
    match validate::parse(doc, format) {
        Ok(d) => validate::check(&d, fonts, images),
        Err(e) => vec![e],
    }
}

/// Validates and renders a document to PDF bytes. Never renders a partial
/// document: any validation problem is returned as an error.
///
/// # Errors
///
/// [`InkoxideError::Validation`] with the first problem [`validate`] would
/// report, or another variant when a resource fails to load or the worker
/// pool cannot be built.
pub fn render(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: &ImageStore,
) -> Result<Vec<u8>, InkoxideError> {
    let _span = tracing::debug_span!("render", bytes = doc.len()).entered();
    match options.threads {
        None => render_inner(doc, format, options, fonts, images),
        Some(n) => {
            let pool = pool::get(n)?;
            pool.install(|| render_inner(doc, format, options, fonts, images))
        }
    }
}

fn render_inner(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: &ImageStore,
) -> Result<Vec<u8>, InkoxideError> {
    let document = validate::parse(doc, format)?;
    if let Some(first) = validate::check(&document, fonts, images).into_iter().next() {
        return Err(first.into());
    }

    let creation_date = match &options.creation_date {
        Some(s) => Some(date::parse_rfc3339(s).ok_or_else(|| {
            InkoxideError::Validation(ValidationError {
                code: ValidationCode::InvalidValue,
                path: "options.creationDate".into(),
                message: format!("`{s}` is not an RFC 3339 timestamp"),
            })
        })?),
        // Already validated.
        None => document
            .meta
            .creation_date
            .as_deref()
            .and_then(date::parse_rfc3339),
    };

    let font_set = FontSet::load(&document, fonts)?;
    let mut text = TextCache::new(
        RustybuzzShaper::new(&font_set),
        document.meta.lang.as_deref(),
    );
    let image_set = images::ImageSet::load(&document.pages, images, &images::ImageCrateDecoder)?;
    let mut sink = PdfWriterSink {
        compression: options.compress.then_some(options.compression_level.min(9)),
        subset_fonts: options.subset_fonts,
        pdf_a: options.pdf_a,
    };
    let (pages, marks) = pipeline::run(&document, &font_set, &mut text, &image_set, |page| {
        sink.encode_page(&page, &font_set)
    })?;
    let unicode: UnicodeMap = text.shaper.unicode;

    sink.write(&DocumentOut {
        pages: &pages,
        meta: &document.meta,
        creation_date,
        fonts: &font_set,
        unicode: &unicode,
        images: &image_set,
        marks: &marks,
    })
}

/// Raw serde message of a document that fails to parse (test support for
/// the schema-parity check; not part of the stable API).
#[doc(hidden)]
pub fn debug_raw_parse_error(doc: &[u8]) -> Option<String> {
    serde_json::from_slice::<schema::Document>(doc)
        .err()
        .map(|e| e.to_string())
}
