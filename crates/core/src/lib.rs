//! Inkoxide: a native PDF layout and rendering engine.
//!
//! ```text
//! render(document_tree, options) -> pdf_bytes
//! ```
//!
//! The input is a fully resolved document tree (see
//! `packages/schema/document.v1.json`); the engine knows nothing about HTML, CSS or
//! React. This crate does no I/O: bytes in, bytes out.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod date;
mod error;
mod fonts;
mod forms;
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
pub use text::HyphenationStore;

use fonts::FontSet;
use pdf::{DocumentOut, PdfSink, PdfWriterSink};
use text::{RustybuzzShaper, TextCache, UnicodeMap};

/// Engine version (the crate version).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serialisation of the document tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum InputFormat {
    /// JSON text (UTF-8).
    #[default]
    Json,
    /// `MessagePack` with the same structure as the JSON (maps keyed by the
    /// JSON field names).
    MessagePack,
}

/// Output settings for [`render`]; the document describes content only.
///
/// Fields are public to read and assign; the type is `#[non_exhaustive]`, so
/// build it from [`RenderOptions::default`] and the `with_*` setters:
///
/// ```
/// let options = inkoxide::RenderOptions::default()
///     .with_threads(1)
///     .with_creation_date("2026-01-01T00:00:00Z")
///     .with_pdf_a(true);
/// assert_eq!(options.threads, Some(1));
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RenderOptions {
    /// Flate-compress content streams (default `true`).
    pub compress: bool,
    /// Flate level 0–9 used when `compress` is set (default 6).
    pub compression_level: u32,
    /// RFC 3339 timestamp; overrides `meta.creationDate`. With neither set
    /// no date is written, so output is byte-for-byte deterministic.
    pub creation_date: Option<String>,
    /// Worker threads for shaping, page painting and compression. `None`
    /// uses rayon's global pool (all cores, shared by every render in the
    /// process). `Some(n)` uses a pool of `n` threads, shared by every
    /// render that asks for `n`. `Some(1)` runs the whole render on the
    /// calling thread with no parallelism inside it, so concurrent
    /// one-thread renders run side by side on their own threads.
    pub threads: Option<usize>,
    /// Embed only the glyphs a document uses (default `true`). Off embeds
    /// whole font programs, for debugging.
    pub subset_fonts: bool,
    /// Write PDF/A-2b (default `false`): XMP metadata, an sRGB output
    /// intent and a file identifier; CMYK JPEGs are converted to RGB and
    /// `.notdef` glyphs are left out.
    pub pdf_a: bool,
    /// Hyphenation patterns registered at runtime; they win over the
    /// built-in ones for their language. Default: none.
    pub hyphenation: HyphenationStore,
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
            hyphenation: HyphenationStore::new(),
        }
    }
}

impl RenderOptions {
    /// Sets [`compress`](Self::compress).
    #[must_use]
    pub fn with_compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Sets [`compression_level`](Self::compression_level) (clamped to 9).
    #[must_use]
    pub fn with_compression_level(mut self, level: u32) -> Self {
        self.compression_level = level;
        self
    }

    /// Sets [`creation_date`](Self::creation_date).
    #[must_use]
    pub fn with_creation_date(mut self, date: impl Into<String>) -> Self {
        self.creation_date = Some(date.into());
        self
    }

    /// Sets [`threads`](Self::threads); `None` uses the shared pool.
    #[must_use]
    pub fn with_threads(mut self, threads: impl Into<Option<usize>>) -> Self {
        self.threads = threads.into();
        self
    }

    /// Sets [`subset_fonts`](Self::subset_fonts).
    #[must_use]
    pub fn with_subset_fonts(mut self, subset: bool) -> Self {
        self.subset_fonts = subset;
        self
    }

    /// Sets [`pdf_a`](Self::pdf_a).
    #[must_use]
    pub fn with_pdf_a(mut self, pdf_a: bool) -> Self {
        self.pdf_a = pdf_a;
        self
    }

    /// Sets [`hyphenation`](Self::hyphenation).
    #[must_use]
    pub fn with_hyphenation(mut self, store: HyphenationStore) -> Self {
        self.hyphenation = store;
        self
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
        None => render_inner(doc, format, options, fonts, images, true),
        // Outside a pool, rayon would use the global one: run every
        // parallel section in sequence instead, so concurrent one-thread
        // renders each keep to their own thread.
        Some(0 | 1) => render_inner(doc, format, options, fonts, images, false),
        Some(n) => {
            let pool = pool::get(n)?;
            pool.install(|| render_inner(doc, format, options, fonts, images, true))
        }
    }
}

fn render_inner(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: &ImageStore,
    parallel: bool,
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
    let hyphenator = text::ResolvedHyphenator {
        store: &options.hyphenation,
    };
    let mut shaper = RustybuzzShaper::new(&font_set)?;
    shaper.hyphenator = &hyphenator;
    let mut text = TextCache::new(shaper, document.meta.lang.as_deref());
    let image_set = images::ImageSet::load(&document.pages, images, &images::ImageCrateDecoder)?;
    let mut sink = PdfWriterSink {
        compression: options.compress.then_some(options.compression_level.min(9)),
        subset_fonts: options.subset_fonts,
        pdf_a: options.pdf_a,
        parallel,
    };
    let (pages, marks) = pipeline::run(
        &document,
        &font_set,
        &mut text,
        &image_set,
        parallel,
        |page| sink.encode_page(&page, &font_set),
    )?;
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
