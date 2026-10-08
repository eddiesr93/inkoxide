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
// TODO(M5): remove once the pipeline consumes the whole schema.
#![allow(dead_code)]

mod date;
mod error;
mod fonts;
mod images;
mod paint;
mod pdf;
mod schema;
mod validate;

pub use error::{InkoxideError, ValidationCode, ValidationError};
pub use fonts::FontStore;
pub use images::ImageStore;
pub use schema::SCHEMA_VERSION;

use paint::{Op, PageContent};
use pdf::{DocumentOut, PdfSink, PdfWriterSink};

/// Engine version (the crate version).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serialisation of the document tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputFormat {
    #[default]
    Json,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Flate-compress content streams (default `true`).
    pub compress: bool,
    /// Flate level 0–9 used when `compress` is set (default 6).
    pub compression_level: u32,
    /// RFC 3339 timestamp; overrides `meta.creationDate`. With neither set
    /// no date is written, so output is byte-for-byte deterministic.
    pub creation_date: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            compress: true,
            compression_level: 6,
            creation_date: None,
        }
    }
}

/// Validates a document against the schema and the registered resources.
/// Returns every problem found (capped at 100); empty means renderable.
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
pub fn render(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: &ImageStore,
) -> Result<Vec<u8>, InkoxideError> {
    let _span = tracing::debug_span!("render", bytes = doc.len()).entered();
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

    // M0: one physical page per page template, background only.
    let pages: Vec<PageContent> = document
        .pages
        .iter()
        .map(|spec| {
            let mut page = PageContent::new(spec.size.width, spec.size.height);
            if let Some(bg) = spec.background {
                page.ops.push(Op::FillRect {
                    rect: paint::Rect::new(0.0, 0.0, spec.size.width, spec.size.height),
                    color: bg,
                });
            }
            page
        })
        .collect();

    let mut sink = PdfWriterSink {
        compression: options.compress.then_some(options.compression_level.min(9)),
    };
    sink.write(&DocumentOut {
        pages: &pages,
        meta: &document.meta,
        creation_date,
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
