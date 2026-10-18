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

mod attachments;
mod date;
mod encryption;
mod error;
mod flate;
mod fonts;
mod forms;
mod geom;
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
mod tag;
#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
mod test_timing;
mod text;
mod validate;

pub use attachments::AttachmentStore;
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
use pdf::{DocumentOut, PdfWriterSink, Profile};
use text::{RustybuzzShaper, TextCache, UnicodeMap};

/// Engine version (the crate version).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The message of the `PARSE_ERROR` for MessagePack input to a build without
/// the `msgpack` feature, which bindings built that way give too.
#[doc(hidden)]
pub const JSON_ONLY: &str = "this build reads JSON only, not MessagePack";

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
/// The "b" (basic) levels make the pages look the same in every reader;
/// the "a" (accessible) levels are tagged as well, with a structure tree
/// (see [`RenderOptions::tagged`]).
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
    /// PDF/A-2a: PDF/A-2b, tagged.
    A2a,
    /// PDF/A-3a: PDF/A-3b, tagged.
    A3a,
}

impl PdfALevel {
    /// Every level, from the oldest part.
    pub const ALL: [Self; 5] = [Self::A1b, Self::A2b, Self::A2a, Self::A3b, Self::A3a];

    /// The level's short name, as the Node binding and the CLI take it:
    /// `"1b"`, `"2b"`, `"2a"`, `"3b"`, `"3a"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A1b => "1b",
            Self::A2b => "2b",
            Self::A2a => "2a",
            Self::A3b => "3b",
            Self::A3a => "3a",
        }
    }

    /// The part of ISO 19005: 1, 2 or 3.
    pub(crate) fn part(self) -> u8 {
        match self {
            Self::A1b => 1,
            Self::A2b | Self::A2a => 2,
            Self::A3b | Self::A3a => 3,
        }
    }

    /// Whether the level is "a" (accessible): tagged.
    pub(crate) fn accessible(self) -> bool {
        matches!(self, Self::A2a | Self::A3a)
    }

    /// The level named `s` ([`as_str`](Self::as_str)).
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str() == s)
    }
}

/// Parses a level's short name (`"1b"`, `"2b"`, `"2a"`, `"3b"`, `"3a"`),
/// with an error that lists them.
///
/// ```
/// use inkoxide::PdfALevel;
/// assert_eq!("3b".parse(), Ok(PdfALevel::A3b));
/// let err = "4".parse::<PdfALevel>().unwrap_err();
/// assert_eq!(
///     err.to_string(),
///     "unsupported PDF/A level `4` (expected one of 1b, 2b, 2a, 3b, 3a)"
/// );
/// ```
impl std::str::FromStr for PdfALevel {
    type Err = ParsePdfALevelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_name(s).ok_or_else(|| ParsePdfALevelError(s.to_string()))
    }
}

/// A name that is not a PDF/A level, from [`PdfALevel`]'s `FromStr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePdfALevelError(String);

impl std::fmt::Display for ParsePdfALevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = PdfALevel::ALL.iter().map(|l| l.as_str()).collect();
        write!(
            f,
            "unsupported PDF/A level `{}` (expected one of {})",
            self.0,
            names.join(", ")
        )
    }
}

impl std::error::Error for ParsePdfALevelError {}

/// The part of PDF/UA (ISO 14289) [`RenderOptions::pdf_ua`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum PdfUaPart {
    /// PDF/UA-1 (ISO 14289-1), on PDF 1.7: it combines with PDF/A.
    #[default]
    Ua1,
    /// PDF/UA-2 (ISO 14289-2:2024), on PDF 2.0: structure elements in
    /// PDF 2.0's namespace and links to structure destinations. PDF/A-1,
    /// -2 and -3 are older than PDF 2.0, so it does not combine with
    /// them.
    Ua2,
}

impl PdfUaPart {
    /// The part's short name, as the Node binding and the CLI take it:
    /// `"1"` or `"2"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ua1 => "1",
            Self::Ua2 => "2",
        }
    }

    /// The part named `s` ([`as_str`](Self::as_str)).
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        [Self::Ua1, Self::Ua2].into_iter().find(|p| p.as_str() == s)
    }
}

/// Parses a part's short name, `"1"` or `"2"`, with an error that lists
/// them.
///
/// ```
/// use inkoxide::PdfUaPart;
/// assert_eq!("2".parse(), Ok(PdfUaPart::Ua2));
/// let err = "3".parse::<PdfUaPart>().unwrap_err();
/// assert_eq!(err.to_string(), "unsupported PDF/UA part `3` (expected 1 or 2)");
/// ```
impl std::str::FromStr for PdfUaPart {
    type Err = ParsePdfUaPartError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_name(s).ok_or_else(|| ParsePdfUaPartError(s.to_string()))
    }
}

/// A name that is not a PDF/UA part, from [`PdfUaPart`]'s `FromStr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePdfUaPartError(String);

impl std::fmt::Display for ParsePdfUaPartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported PDF/UA part `{}` (expected 1 or 2)", self.0)
    }
}

impl std::error::Error for ParsePdfUaPartError {}

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
    /// At PDF/A-1b, which forbids transparency, draw images whose alpha is
    /// partial (neither only clear nor only opaque) on white instead of
    /// refusing them (default `false`). Other levels and plain PDF keep
    /// the alpha. Binary alpha needs no option: PDF/A-1b writes it as a
    /// stencil mask.
    pub flatten_alpha: bool,
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
    /// naming it; one that does not renders as if it were registered. A
    /// fallback is tried for a character only once the font of its run is
    /// registered: until then the render names the run's font alone.
    pub deferred_fonts: Vec<String>,
    /// Write tagged PDF (default `false`): a structure tree that gives the
    /// reading order (document order) and what each part of the content
    /// is (paragraphs, headings, figures with their `alt`, tables, lists),
    /// and marks the rest as decoration, for screen readers, reflow and
    /// extraction. The PDF/A "a" levels and [`pdf_ua`](Self::pdf_ua) imply
    /// it.
    pub tagged: bool,
    /// Write PDF/UA (ISO 14289, default `false`), of part
    /// [`pdf_ua_part`](Self::pdf_ua_part): tagged, with the metadata,
    /// title, language and alternative texts it requires; a document
    /// without them fails with `UA_CONFLICT`.
    pub pdf_ua: bool,
    /// The part of PDF/UA written when [`pdf_ua`](Self::pdf_ua) is set
    /// (default [`PdfUaPart::Ua1`]).
    pub pdf_ua_part: PdfUaPart,
    /// Files that the document's `attachments` name by `src` (default:
    /// none). A `src` missing from here is `ATTACHMENT_NOT_REGISTERED`, in
    /// [`render`] and [`validate_with_options`]; [`validate`] does not see
    /// them.
    pub attachments: AttachmentStore,
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
            flatten_alpha: false,
            hyphenation: HyphenationStore::new(),
            encryption: None,
            deferred_fonts: Vec::new(),
            attachments: AttachmentStore::new(),
            tagged: false,
            pdf_ua: false,
            pdf_ua_part: PdfUaPart::default(),
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

    /// Sets [`flatten_alpha`](Self::flatten_alpha).
    #[must_use]
    pub fn with_flatten_alpha(mut self, flatten: bool) -> Self {
        self.flatten_alpha = flatten;
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

    /// Sets [`tagged`](Self::tagged).
    #[must_use]
    pub fn with_tagged(mut self, tagged: bool) -> Self {
        self.tagged = tagged;
        self
    }

    /// Sets [`pdf_ua`](Self::pdf_ua).
    #[must_use]
    pub fn with_pdf_ua(mut self, pdf_ua: bool) -> Self {
        self.pdf_ua = pdf_ua;
        self
    }

    /// Writes PDF/UA of part `part`: sets
    /// [`pdf_ua_part`](Self::pdf_ua_part) and [`pdf_ua`](Self::pdf_ua).
    #[must_use]
    pub fn with_pdf_ua_part(mut self, part: PdfUaPart) -> Self {
        self.pdf_ua = true;
        self.pdf_ua_part = part;
        self
    }

    /// Whether the output is tagged: [`tagged`](Self::tagged), PDF/UA or
    /// an accessible PDF/A level.
    fn is_tagged(&self) -> bool {
        self.tagged || self.pdf_ua || (self.pdf_a && self.pdf_a_level.accessible())
    }

    /// What the writer is asked to conform to.
    fn profile(&self) -> Profile {
        Profile::new(self.pdf_a.then_some(self.pdf_a_level))
            .with_ua(self.pdf_ua.then_some(self.pdf_ua_part))
            .with_flatten_alpha(self.flatten_alpha)
    }

    /// The options' own problems, as [`render`] reports them: a
    /// `creation_date` that is not RFC 3339, PDF/UA-2 with PDF/A, and
    /// encryption that is not valid or that PDF/A or PDF/UA does not allow. Otherwise the
    /// creation date and the encryption parameters.
    fn checked(
        &self,
    ) -> Result<(Option<date::Timestamp>, Option<pdf::crypt::Params>), ValidationError> {
        let invalid = |path: &str, message: String| ValidationError {
            code: ValidationCode::InvalidValue,
            path: path.into(),
            message,
        };
        let creation_date = match &self.creation_date {
            Some(s) => Some(date::parse_rfc3339(s).ok_or_else(|| {
                invalid(
                    "options.creationDate",
                    format!("`{s}` is not an RFC 3339 timestamp"),
                )
            })?),
            None => None,
        };
        if self.pdf_ua && self.pdf_ua_part == PdfUaPart::Ua2 && self.pdf_a {
            return Err(invalid(
                "options.pdfUA",
                "PDF/UA-2 is PDF 2.0, which PDF/A-1, -2 and -3 are older than".into(),
            ));
        }
        let encryption = match &self.encryption {
            None => None,
            Some(_) if self.pdf_a => {
                return Err(invalid(
                    "options.encryption",
                    "PDF/A does not allow encryption".into(),
                ));
            }
            // Assistive technology reads the content (ISO 14289-1, 7.16).
            Some(e)
                if self.pdf_ua
                    && e.permissions
                        .as_ref()
                        .is_some_and(|p| !p.content_accessibility) =>
            {
                return Err(invalid(
                    "options.encryption.permissions.contentAccessibility",
                    "PDF/UA needs content accessibility on".into(),
                ));
            }
            Some(e) => Some(e.params()?),
        };
        Ok((creation_date, encryption))
    }

    /// Sets [`attachments`](Self::attachments).
    #[must_use]
    pub fn with_attachments(mut self, store: AttachmentStore) -> Self {
        self.attachments = store;
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
/// level does not allow ([`ValidationCode::PdfaConflict`]), which
/// [`validate_with_options`] reports, on the page limit or the limit on
/// content repeated on every page, or when a font cannot be shaped or
/// embedded.
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
        Ok(d) => validate::check(&d, fonts, &[], images, None, Profile::default()),
        Err(e) => vec![e],
    }
}

/// [`validate`] for a render with `options`: also reports what they do
/// not allow, as [`render`] would. That is what the PDF/A level or PDF/UA
/// forbids ([`ValidationCode::PdfaConflict`],
/// [`ValidationCode::UaConflict`]), an attachment's `src` missing from
/// `options.attachments`, and the options' own problems (an invalid
/// `creation_date` or encryption), at paths that start with `options.`.
/// Fonts in `options.deferred_fonts` may be missing. With `images`
/// `None`, image ids are not looked up, as in [`validate_without_images`].
/// [`render`] can still fail on what only layout finds: the page limit,
/// the limit on content repeated on every page, a font that cannot be
/// shaped or embedded, or at PDF/A-1b a colour glyph that needs
/// transparency.
#[must_use]
pub fn validate_with_options(
    doc: &[u8],
    format: InputFormat,
    options: &RenderOptions,
    fonts: &FontStore,
    images: Option<&ImageStore>,
) -> Vec<ValidationError> {
    let mut errors = match validate::parse(doc, format) {
        Ok(d) => validate::check(
            &d,
            fonts,
            &options.deferred_fonts,
            images,
            Some(&options.attachments),
            options.profile(),
        ),
        Err(e) => return vec![e],
    };
    if let Err(e) = options.checked() {
        if errors.len() < validate::MAX_ERRORS {
            errors.push(e);
        }
    }
    errors
}

/// Empties the process-wide caches renders share: decoded images, font
/// subsets, and SVG converted with its text. Later renders fill them
/// again, with the same output. In WebAssembly the memory goes back to the
/// instance for reuse, not to the host.
pub fn clear_caches() {
    // Every process-wide cache (a `SharedLru` static).
    images::CACHE.clear();
    images::svg::TEXT_TREES.clear();
    pdf::SUBSETS.clear();
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
    // lock poisoned by a panic (lru.rs, images/svg/limits.rs).
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
    let profile = options.profile();
    let document = validate::parse(doc, format)?;
    // A deferred font may be missing: it stands in until a render needs it.
    let checked = validate::check(
        &document,
        fonts,
        &options.deferred_fonts,
        Some(images),
        Some(&options.attachments),
        profile,
    );
    if let Some(first) = checked.into_iter().next() {
        return Err(first.into());
    }

    let (option_date, encryption) = options.checked()?;
    // Already validated.
    let creation_date = option_date.or_else(|| {
        document
            .meta
            .creation_date
            .as_deref()
            .and_then(date::parse_rfc3339)
    });

    let mut font_set = FontSet::load_with(&document, fonts, &options.deferred_fonts)?;
    let hyphenator = text::ResolvedHyphenator::new(&options.hyphenation);
    let mut image_set =
        images::ImageSet::load(&document.pages, images, &images::ImageCrateDecoder)?;
    image_set.load_emoji(&document.emoji_images, images, &images::ImageCrateDecoder)?;
    // The instances of variable fonts text asks for besides the declared
    // ones, before the shaper loads a face for each.
    font_set.add_variants(text::font_asks(&document, &font_set, &image_set));
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
    let compression = options.compress.then_some(options.compression_level.min(9));
    let mut sink = PdfWriterSink {
        compression,
        subset_fonts: options.subset_fonts,
        profile,
        encryption,
        parallel,
    };
    let plan = options
        .is_tagged()
        .then(|| tag::Plan::new(&document, options.profile().pdf_2()).with_parted(&image_set));
    let laid_out = pipeline::run(
        &document,
        &font_set,
        &mut text,
        &mut image_set,
        parallel,
        (profile, plan.as_ref()),
        |page| sink.encode_page(&page, &font_set, plan.as_ref()),
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
    // After layout, which a missing font fails: compressed once, though an
    // encrypted render writes the document twice.
    let files = attachments::prepare(
        &document.attachments,
        &options.attachments,
        compression,
        creation_date,
        parallel,
    )?;

    sink.write(&DocumentOut {
        pages: &pages,
        meta: &document.meta,
        creation_date,
        fonts: &font_set,
        unicode: &unicode,
        images: &image_set,
        colors: &colors,
        marks: &marks,
        files: &files,
        plan: plan.as_ref(),
    })
}
