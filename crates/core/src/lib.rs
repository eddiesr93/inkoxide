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
mod encryption;
mod error;
mod flate;
mod fonts;
mod forms;
mod icons;
mod images;
mod layout;
mod lru;
mod notes;
mod paginate;
mod paint;
mod pdf;
mod pipeline;
mod pool;
mod schema;
#[cfg(test)]
mod test_fixtures;
mod text;
mod validate;

pub use encryption::{Encryption, Permissions, Printing};
pub use error::{InkoxideError, NotLoaded, ValidationCode, ValidationError};
pub use fonts::FontStore;
pub use images::ImageStore;
pub use schema::SCHEMA_VERSION;
pub use text::{builtin_hyphenation_file, HyphenationStore};

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use fonts::FontSet;
use pdf::{DocumentOut, PdfSink, PdfWriterSink, Profile};
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
    /// JSON field names). A build without the `msgpack` feature (on by
    /// default) reports it as a parse error.
    MessagePack,
}

/// The PDF/A part and conformance level [`RenderOptions::pdf_a`] writes.
/// Every level is "b" (basic): the pages look the same in every reader,
/// with no structure tree for accessibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum PdfALevel {
    /// PDF/A-1b (ISO 19005-1), based on PDF 1.4: the strictest level,
    /// for archives that ask for it.
    A1b,
    /// PDF/A-2b (ISO 19005-2), based on PDF 1.7.
    #[default]
    A2b,
    /// PDF/A-3b (ISO 19005-3): PDF/A-2b that may also carry embedded
    /// files of any kind.
    A3b,
}

impl PdfALevel {
    /// Every level, from the oldest part.
    pub const ALL: [Self; 3] = [Self::A1b, Self::A2b, Self::A3b];

    /// The level's short name, as the Node binding and the CLI take it:
    /// `"1b"`, `"2b"`, `"3b"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A1b => "1b",
            Self::A2b => "2b",
            Self::A3b => "3b",
        }
    }

    /// The level named `s` ([`as_str`](Self::as_str)).
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str() == s)
    }
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
    /// render that asks for `n`, and kept for the life of the process.
    /// Pools are at most 256 threads: a larger `n` uses the 256-thread
    /// pool. `Some(1)` (and `Some(0)`) runs the whole render on the
    /// calling thread with no parallelism inside it, so concurrent
    /// one-thread renders run side by side on their own threads.
    pub threads: Option<usize>,
    /// Embed only the glyphs a document uses (default `true`). Off embeds
    /// whole font programs, for debugging.
    pub subset_fonts: bool,
    /// Write PDF/A (default `false`), at [`pdf_a_level`](Self::pdf_a_level):
    /// XMP metadata, an sRGB output intent and a file identifier; CMYK
    /// JPEGs are converted to RGB and `.notdef` glyphs are left out.
    pub pdf_a: bool,
    /// The PDF/A level written when [`pdf_a`](Self::pdf_a) is set (default
    /// [`PdfALevel::A2b`]).
    pub pdf_a_level: PdfALevel,
    /// Hyphenation patterns registered at runtime; they win over the
    /// built-in ones for their language. Default: none.
    pub hyphenation: HyphenationStore,
    /// Encrypt the PDF with passwords and permissions (default `None`).
    /// Not with [`pdf_a`](Self::pdf_a), which forbids encryption.
    pub encryption: Option<Encryption>,
    /// Ids of fonts the document declares that the caller gives only when
    /// a render needs them (default: none), such as fallback fonts that a
    /// browser would otherwise download for every document. One that is
    /// not registered may be: a render that draws with it, or might (it is
    /// tried for a character), fails with [`InkoxideError::NotLoaded`]
    /// naming it; one that does not renders as if it were registered.
    pub deferred_fonts: Vec<String>,
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
            pdf_a_level: PdfALevel::default(),
            hyphenation: HyphenationStore::new(),
            encryption: None,
            deferred_fonts: Vec::new(),
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

    /// Sets [`compression_level`](Self::compression_level). The setter keeps
    /// the value; a render uses 9 for a larger one.
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

    /// Writes PDF/A at `level`: sets [`pdf_a_level`](Self::pdf_a_level)
    /// and [`pdf_a`](Self::pdf_a).
    #[must_use]
    pub fn with_pdf_a_level(mut self, level: PdfALevel) -> Self {
        self.pdf_a = true;
        self.pdf_a_level = level;
        self
    }

    /// Sets [`deferred_fonts`](Self::deferred_fonts).
    #[must_use]
    pub fn with_deferred_fonts(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.deferred_fonts = ids.into_iter().map(Into::into).collect();
        self
    }

    /// Sets [`hyphenation`](Self::hyphenation).
    #[must_use]
    pub fn with_hyphenation(mut self, store: HyphenationStore) -> Self {
        self.hyphenation = store;
        self
    }

    /// Sets [`encryption`](Self::encryption).
    #[must_use]
    pub fn with_encryption(mut self, encryption: Encryption) -> Self {
        self.encryption = Some(encryption);
        self
    }
}

/// Validates a document against the schema and the registered resources:
/// every font must be registered, and every image must exist and decode.
/// Returns every problem found (capped at 100). Empty means the document
/// is valid; [`render`] can still fail on the options, on what the PDF/A
/// level does not allow ([`ValidationCode::PdfaConflict`]), on the page
/// limit or the limit on content repeated on every page, or when a font
/// cannot be shaped or embedded.
#[must_use]
pub fn validate(
    doc: &[u8],
    format: InputFormat,
    fonts: &FontStore,
    images: &ImageStore,
) -> Vec<ValidationError> {
    validate_with(doc, format, fonts, Some(images))
}

/// [`validate`] for when the images are not at hand: image ids (`src`) are
/// not looked up, so a missing or undecodable image is not reported.
/// Inline images (`data`) are checked as in [`validate`].
#[must_use]
pub fn validate_without_images(
    doc: &[u8],
    format: InputFormat,
    fonts: &FontStore,
) -> Vec<ValidationError> {
    validate_with(doc, format, fonts, None)
}

fn validate_with(
    doc: &[u8],
    format: InputFormat,
    fonts: &FontStore,
    images: Option<&ImageStore>,
) -> Vec<ValidationError> {
    match validate::parse(doc, format) {
        // No profile: `validate` does not know the level.
        Ok(d) => validate::check(&d, fonts, images, Profile::default()),
        Err(e) => vec![e],
    }
}

/// Empties the process-wide caches renders share: decoded images, font
/// subsets, and SVG converted with its text. Later renders fill them
/// again, with the same output. In WebAssembly the memory goes back to the
/// instance for reuse, not to the host.
pub fn clear_caches() {
    images::clear_caches();
    pdf::clear_subsets();
}

/// Validates and renders a document to PDF bytes. Never renders a partial
/// document: any validation problem is returned as an error.
///
/// # Errors
///
/// [`InkoxideError::Validation`] with the first problem [`validate`] would
/// report, or another variant when a resource fails to load or the worker
/// pool cannot be built. A panic inside the engine is returned as
/// [`InkoxideError::Font`] when it happened while shaping text (naming the
/// font), and as [`InkoxideError::Internal`] otherwise.
pub fn render(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: &ImageStore,
) -> Result<Vec<u8>, InkoxideError> {
    let _span = tracing::debug_span!("render", bytes = doc.len()).entered();
    // Rayon resumes a worker's panic, with its payload, in the thread that
    // waits for it, so this also catches panics in parallel sections. The
    // inputs are only read, and the process-wide caches recover from a
    // lock poisoned by a panic (lru.rs, images/svg.rs).
    catch_unwind(AssertUnwindSafe(|| match options.threads {
        None => render_inner(doc, format, options, fonts, images, true),
        // Outside a pool, rayon would use the global one: run every
        // parallel section in sequence instead, so concurrent one-thread
        // renders each keep to their own thread.
        Some(0 | 1) => render_inner(doc, format, options, fonts, images, false),
        Some(n) => {
            let pool = pool::get(n)?;
            pool.install(|| render_inner(doc, format, options, fonts, images, true))
        }
    }))
    .unwrap_or_else(|payload| Err(panic_error(payload.as_ref())))
}

/// The error a render returns for a panic with `payload`. A panic while
/// shaping is a font error instead ([`text::TextCache::shaping_error`]).
fn panic_error(payload: &(dyn Any + Send)) -> InkoxideError {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown cause");
    InkoxideError::Internal(format!("the engine panicked: {message}"))
}

fn render_inner(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: &ImageStore,
    parallel: bool,
) -> Result<Vec<u8>, InkoxideError> {
    let profile = Profile::new(options.pdf_a.then_some(options.pdf_a_level));
    let document = validate::parse(doc, format)?;
    // A deferred font may be missing: it stands in until a render needs it.
    let deferred = |e: &ValidationError| {
        e.code == ValidationCode::FontNotRegistered
            && document.fonts.iter().enumerate().any(|(i, f)| {
                e.path == format!("fonts[{i}].id") && options.deferred_fonts.contains(&f.id)
            })
    };
    if let Some(first) = validate::check(&document, fonts, Some(images), profile)
        .into_iter()
        .find(|e| !deferred(e))
    {
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

    let encryption = match &options.encryption {
        None => None,
        Some(_) if options.pdf_a => {
            return Err(InkoxideError::Validation(ValidationError {
                code: ValidationCode::InvalidValue,
                path: "options.encryption".into(),
                message: "PDF/A does not allow encryption".into(),
            }));
        }
        Some(e) => Some(e.params()?),
    };

    let font_set = FontSet::load_with(&document, fonts, &options.deferred_fonts)?;
    let hyphenator = text::ResolvedHyphenator::new(&options.hyphenation);
    let mut image_set =
        images::ImageSet::load(&document.pages, images, &images::ImageCrateDecoder)?;
    image_set.load_emoji(&document.emoji_images, images, &images::ImageCrateDecoder)?;
    let shaper = RustybuzzShaper::new(&font_set)?
        .with_hyphenator(&hyphenator)
        .with_objects(Arc::new(image_set.inline_objects()));
    let mut text = TextCache::new(shaper, document.meta.lang.as_deref());
    // SVG text is shaped with the document's text, before layout.
    let svg_text = image_set.lay_out_text(text.shaper());
    if let Some(e) = text.shaping_error() {
        return Err(e);
    }
    svg_text?;
    let mut sink = PdfWriterSink {
        compression: options.compress.then_some(options.compression_level.min(9)),
        subset_fonts: options.subset_fonts,
        profile,
        encryption,
        parallel,
    };
    let laid_out = pipeline::run(
        &document,
        &font_set,
        &mut text,
        &mut image_set,
        parallel,
        profile,
        |page| sink.encode_page(&page, &font_set),
    );
    // Text left unshaped may have failed layout another way.
    if let Some(e) = text.shaping_error() {
        return Err(e);
    }
    // Text that needed a deferred font, or built-in hyphenation patterns
    // this build was not given, was laid out without them: render again
    // once they are.
    let not_loaded = NotLoaded {
        fonts: font_set.not_loaded(),
        hyphenation: hyphenator.not_loaded(),
    };
    if not_loaded != NotLoaded::default() {
        return Err(InkoxideError::NotLoaded(not_loaded));
    }
    let (pages, marks, colors) = laid_out?;
    let unicode: UnicodeMap = text.into_unicode();

    sink.write(&DocumentOut {
        pages: &pages,
        meta: &document.meta,
        creation_date,
        fonts: &font_set,
        unicode: &unicode,
        images: &image_set,
        colors: &colors,
        marks: &marks,
    })
}
