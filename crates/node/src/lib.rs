//! Node.js binding for Inkoxide (npm `inkoxide`). A thin adapter: argument
//! conversion, the process-wide font registry, async scheduling on the
//! libuv pool, and error mapping. All engine logic lives in `inkoxide`.
//!
//! Errors cross the boundary as `Error`s whose message is a JSON object
//! `{ code, path, message }`; `packages/inkoxide/index.js` unpacks it into an
//! `InkoxideError` with those three properties.

// napi exports take owned JS values and document their errors in
// `packages/inkoxide/index.d.ts`; napi objects hold JS handles that have no `Debug`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    missing_debug_implementations
)]

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{LazyLock, RwLock};

use inkoxide::{FontStore, HyphenationStore, ImageStore, InputFormat};
use inkoxide_binding::{Failure, PdfA};
use napi::bindgen_prelude::{AsyncTask, Buffer, Either, Uint8Array};
use napi::{Env, Error, Result, Task};
use napi_derive::napi;

/// Fonts registered through `registerFont`, shared by all renders.
static FONTS: LazyLock<RwLock<FontStore>> = LazyLock::new(Default::default);
/// Patterns registered through `registerHyphenationPatterns`, shared by all
/// renders.
static HYPHENATION: LazyLock<RwLock<HyphenationStore>> = LazyLock::new(Default::default);

/// `failure` as a JS error, which `index.js` unpacks.
fn js_error(failure: impl Into<Failure>) -> Error {
    Error::from_reason(failure.into().to_json())
}

/// Runs engine code, turning a panic into an `INTERNAL` error instead of
/// letting it unwind into Node.
fn catch_panic<T>(f: impl FnOnce() -> T) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f))
        .map_err(|_| js_error(Failure::internal("the engine panicked")))
}

/// Largest `threads`: the engine's own cap on a pool size.
const MAX_THREADS: u32 = 256;

fn fonts_snapshot() -> FontStore {
    FONTS.read().map(|f| f.clone()).unwrap_or_default()
}

/// The bytes of a document or of hyphenation patterns, passed as bytes or
/// as a string.
fn input_bytes(input: Either<Uint8Array, String>) -> Vec<u8> {
    match input {
        Either::A(b) => b.to_vec(),
        Either::B(s) => s.into_bytes(),
    }
}

#[napi(object)]
pub struct RenderOptions {
    /// `"json"` (default) or `"msgpack"`.
    pub format: Option<String>,
    /// Flate-compress content streams (default `true`).
    pub compress: Option<bool>,
    /// Flate level 0–9 used when `compress` is set (default 6). A JS number
    /// (not `u32`, which napi would wrap and truncate), checked in
    /// [`RenderJob::new`].
    pub compression_level: Option<f64>,
    /// RFC 3339 timestamp; a fixed value gives byte-identical output.
    pub creation_date: Option<String>,
    /// Worker threads for this render (default: all cores); 1 to
    /// [`MAX_THREADS`]. A JS number, as `compression_level`.
    pub threads: Option<f64>,
    /// Embed only the glyphs used (default `true`).
    pub subset_fonts: Option<bool>,
    /// Write PDF/A (default `false`): `true` or a level, `"1b"`, `"2b"` or
    /// `"3b"`; `true` is `"2b"`.
    pub pdf_a: Option<Either<bool, String>>,
    /// Image bytes by id, for this document only.
    pub images: Option<HashMap<String, Uint8Array>>,
    /// Encrypt the PDF (default: not encrypted).
    pub encryption: Option<EncryptionOptions>,
}

/// `options.encryption`: mirrors Rust `Encryption`.
#[napi(object)]
pub struct EncryptionOptions {
    pub user_password: Option<String>,
    pub owner_password: Option<String>,
    pub permissions: Option<PermissionOptions>,
}

/// `options.encryption.permissions`: mirrors Rust `Permissions`; a field
/// left out keeps its default (all off, `contentAccessibility` on).
#[napi(object)]
pub struct PermissionOptions {
    /// `"lowResolution"` or `"highResolution"`.
    pub printing: Option<String>,
    pub modifying: Option<bool>,
    pub copying: Option<bool>,
    pub annotating: Option<bool>,
    pub filling_forms: Option<bool>,
    pub content_accessibility: Option<bool>,
    pub document_assembly: Option<bool>,
}

#[napi(object)]
pub struct ValidationError {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[napi(object)]
pub struct Version {
    /// Engine (crate) version, e.g. `"0.1.0"`.
    pub engine: String,
    /// The document `version` this engine reads.
    pub schema: u32,
    /// Commit the binding was built from (12 hex digits), or `"unknown"`.
    pub git_sha: String,
}

/// Everything a render needs, owned, so it can move to a worker thread.
pub struct RenderJob {
    doc: Vec<u8>,
    format: InputFormat,
    options: inkoxide::RenderOptions,
    images: ImageStore,
    fonts: FontStore,
}

impl From<PermissionOptions> for inkoxide_binding::PermissionOptions {
    fn from(p: PermissionOptions) -> Self {
        Self {
            printing: p.printing,
            modifying: p.modifying,
            copying: p.copying,
            annotating: p.annotating,
            filling_forms: p.filling_forms,
            content_accessibility: p.content_accessibility,
            document_assembly: p.document_assembly,
        }
    }
}

impl RenderJob {
    fn new(doc: Either<Uint8Array, String>, options: Option<RenderOptions>) -> Result<Self> {
        let mut images = ImageStore::new();
        let mut shared = inkoxide_binding::Options::default();
        if let Some(o) = options {
            shared = inkoxide_binding::Options {
                format: o.format,
                compress: o.compress,
                compression_level: o.compression_level,
                creation_date: o.creation_date,
                threads: o.threads,
                subset_fonts: o.subset_fonts,
                pdf_a: o.pdf_a.map(|p| match p {
                    Either::A(on) => PdfA::On(on),
                    Either::B(level) => PdfA::Level(level),
                }),
                encryption: o.encryption.map(|e| inkoxide_binding::EncryptionOptions {
                    user_password: e.user_password,
                    owner_password: e.owner_password,
                    permissions: e.permissions.map(Into::into),
                }),
            };
            if let Some(map) = o.images {
                for (id, bytes) in map {
                    images.insert(id, bytes.to_vec());
                }
            }
        }
        let hyphenation = HYPHENATION.read().map(|h| h.clone()).unwrap_or_default();
        let (format, core) = shared
            .into_core(hyphenation, MAX_THREADS)
            .map_err(js_error)?;
        Ok(Self {
            doc: input_bytes(doc),
            format,
            options: core,
            images,
            fonts: fonts_snapshot(),
        })
    }

    fn run(&self) -> Result<Vec<u8>> {
        catch_panic(|| {
            inkoxide::render(
                &self.doc,
                self.format,
                &self.options,
                &self.fonts,
                &self.images,
            )
        })?
        .map_err(js_error)
    }
}

impl Task for RenderJob {
    type Output = Vec<u8>;
    type JsValue = Buffer;

    fn compute(&mut self) -> Result<Self::Output> {
        self.run()
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }
}

/// Registers (or replaces) a TrueType, CFF OpenType, WOFF or WOFF2 font for all
/// later renders.
#[napi]
pub fn register_font(id: String, data: Uint8Array) -> Result<()> {
    let bytes = data.to_vec();
    let mut fonts = FONTS
        .write()
        .map_err(|_| js_error(Failure::internal("font registry poisoned")))?;
    // The panic is caught while the guard is still held, so it does not
    // poison the registry, and a font is only inserted once it has parsed.
    catch_panic(|| fonts.register(id, bytes))?.map_err(js_error)
}

/// Removes a registered font; returns whether it existed.
#[napi]
pub fn unregister_font(id: String) -> bool {
    FONTS.write().is_ok_and(|mut f| f.unregister(&id))
}

/// Registers (or replaces) TeX hyphenation patterns for a BCP-47 tag, used
/// by all later renders before the built-in patterns.
#[napi]
pub fn register_hyphenation_patterns(lang: String, data: Either<Uint8Array, String>) -> Result<()> {
    let bytes = input_bytes(data);
    let mut store = HYPHENATION
        .write()
        .map_err(|_| js_error(Failure::internal("hyphenation registry poisoned")))?;
    catch_panic(|| store.register(&lang, &bytes))?.map_err(js_error)
}

/// Removes the patterns registered for exactly `lang`; returns whether
/// there were any.
#[napi]
pub fn unregister_hyphenation_patterns(lang: String) -> bool {
    HYPHENATION.write().is_ok_and(|mut h| h.unregister(&lang))
}

/// Validates a document against the schema and the registered fonts.
/// Images are not available here, so `src` references are not checked
/// unless `images` is given.
#[napi]
pub fn validate(
    doc: Either<Uint8Array, String>,
    format: Option<String>,
    images: Option<HashMap<String, Uint8Array>>,
) -> Result<Vec<ValidationError>> {
    let format =
        inkoxide_binding::input_format(format.as_deref(), "options.format").map_err(js_error)?;
    let bytes = input_bytes(doc);
    let fonts = fonts_snapshot();
    let store = images.map(|images| {
        let mut store = ImageStore::new();
        for (id, b) in images {
            store.insert(id, b.to_vec());
        }
        store
    });
    let errors = catch_panic(|| match &store {
        Some(store) => inkoxide::validate(&bytes, format, &fonts, store),
        None => inkoxide::validate_without_images(&bytes, format, &fonts),
    })?;
    Ok(errors
        .into_iter()
        .map(|e| ValidationError {
            code: e.code.as_str().to_string(),
            path: e.path,
            message: e.message,
        })
        .collect())
}

/// Renders on the libuv thread pool; never blocks the event loop.
#[napi(ts_return_type = "Promise<Buffer>")]
pub fn render(
    doc: Either<Uint8Array, String>,
    options: Option<RenderOptions>,
) -> Result<AsyncTask<RenderJob>> {
    Ok(AsyncTask::new(RenderJob::new(doc, options)?))
}

/// Renders on the calling thread (CLIs, tests).
#[napi]
pub fn render_sync(
    doc: Either<Uint8Array, String>,
    options: Option<RenderOptions>,
) -> Result<Buffer> {
    let job = RenderJob::new(doc, options)?;
    Ok(job.run()?.into())
}

#[napi]
pub fn version() -> Version {
    Version {
        engine: inkoxide::ENGINE_VERSION.to_string(),
        schema: inkoxide::SCHEMA_VERSION,
        git_sha: env!("INKOXIDE_GIT_SHA").to_string(),
    }
}

/// Panics inside `catch_panic`, exactly like an engine bug would, so the
/// tests can check that a panic surfaces as an `INTERNAL` error instead of
/// taking the process down. Not part of the public API.
#[doc(hidden)]
#[napi(js_name = "__testPanic")]
pub fn test_panic() -> Result<()> {
    catch_panic(|| panic!("test panic"))
}
