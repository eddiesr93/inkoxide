//! Node.js binding for Inkoxide (npm `inkoxide`). A thin adapter: argument
//! conversion, the process-wide font registry, async scheduling on the
//! libuv pool, and error mapping. All engine logic lives in `inkoxide`.
//!
//! Errors cross the boundary as `Error`s whose message is a JSON object
//! `{ code, path, message }`; `npm/index.js` unpacks it into an
//! `InkoxideError` with those three properties.

// napi exports take owned JS values and document their errors in
// `npm/index.d.ts`; napi objects hold JS handles that have no `Debug`.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    missing_debug_implementations
)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{LazyLock, RwLock};

use inkoxide::{FontStore, ImageStore, InkoxideError, InputFormat};
use napi::bindgen_prelude::{AsyncTask, Buffer, Either, Uint8Array};
use napi::{Env, Error, Result, Task};
use napi_derive::napi;

/// Fonts registered through `registerFont`, shared by all renders.
static FONTS: LazyLock<RwLock<FontStore>> = LazyLock::new(Default::default);

fn engine_error(code: &str, path: &str, message: &str) -> Error {
    let reason = format!(
        "{{\"code\":{},\"path\":{},\"message\":{}}}",
        json_string(code),
        json_string(path),
        json_string(message)
    );
    Error::from_reason(reason)
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn from_engine(e: InkoxideError) -> Error {
    match &e {
        InkoxideError::Validation(v) => engine_error(v.code.as_str(), &v.path, &v.message),
        other => engine_error(other.code(), "", &other.to_string()),
    }
}

fn invalid_arg(path: &str, message: &str) -> Error {
    engine_error("INVALID_ARGUMENT", path, message)
}

fn fonts_snapshot() -> FontStore {
    FONTS.read().map(|f| f.clone()).unwrap_or_default()
}

fn doc_bytes(doc: Either<Uint8Array, String>) -> Vec<u8> {
    match doc {
        Either::A(b) => b.to_vec(),
        Either::B(s) => s.into_bytes(),
    }
}

fn input_format(format: Option<&str>) -> Result<InputFormat> {
    match format {
        None | Some("json") => Ok(InputFormat::Json),
        Some("msgpack") => Ok(InputFormat::MessagePack),
        Some(other) => Err(invalid_arg(
            "options.format",
            &format!("unsupported format `{other}` (expected \"json\" or \"msgpack\")"),
        )),
    }
}

#[napi(object)]
pub struct RenderOptions {
    /// `"json"` (default) or `"msgpack"`.
    pub format: Option<String>,
    /// Flate-compress content streams (default `true`).
    pub compress: Option<bool>,
    /// Flate level 0–9 used when `compress` is set (default 6).
    pub compression_level: Option<u32>,
    /// RFC 3339 timestamp; a fixed value gives byte-identical output.
    pub creation_date: Option<String>,
    /// Worker threads for this render (default: all cores).
    pub threads: Option<u32>,
    /// Embed only the glyphs used (default `true`).
    pub subset_fonts: Option<bool>,
    /// Write PDF/A-2b (default `false`).
    pub pdf_a: Option<bool>,
    /// Image bytes by id, for this document only.
    pub images: Option<HashMap<String, Uint8Array>>,
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

impl RenderJob {
    fn new(doc: Either<Uint8Array, String>, options: Option<RenderOptions>) -> Result<Self> {
        let mut images = ImageStore::new();
        let mut core = inkoxide::RenderOptions::default();
        let mut format = InputFormat::Json;
        if let Some(o) = options {
            format = input_format(o.format.as_deref())?;
            if let Some(c) = o.compress {
                core.compress = c;
            }
            if let Some(level) = o.compression_level {
                if level > 9 {
                    return Err(invalid_arg(
                        "options.compressionLevel",
                        "must be between 0 and 9",
                    ));
                }
                core.compression_level = level;
            }
            core.creation_date = o.creation_date;
            if let Some(t) = o.threads {
                if t == 0 {
                    return Err(invalid_arg("options.threads", "must be at least 1"));
                }
                core.threads = Some(t as usize);
            }
            if let Some(s) = o.subset_fonts {
                core.subset_fonts = s;
            }
            if let Some(a) = o.pdf_a {
                core.pdf_a = a;
            }
            if let Some(map) = o.images {
                for (id, bytes) in map {
                    images.insert(id, bytes.to_vec());
                }
            }
        }
        Ok(Self {
            doc: doc_bytes(doc),
            format,
            options: core,
            images,
            fonts: fonts_snapshot(),
        })
    }

    fn run(&self) -> Result<Vec<u8>> {
        catch_unwind(AssertUnwindSafe(|| {
            inkoxide::render(
                &self.doc,
                self.format,
                &self.options,
                &self.fonts,
                &self.images,
            )
        }))
        .map_err(|_| engine_error("INTERNAL", "", "the engine panicked"))?
        .map_err(from_engine)
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

/// Registers (or replaces) a TrueType or CFF OpenType font for all later renders.
#[napi]
pub fn register_font(id: String, data: Uint8Array) -> Result<()> {
    let mut fonts = FONTS
        .write()
        .map_err(|_| engine_error("INTERNAL", "", "font registry poisoned"))?;
    fonts.register(id, data.to_vec()).map_err(from_engine)
}

/// Removes a registered font; returns whether it existed.
#[napi]
pub fn unregister_font(id: String) -> bool {
    FONTS.write().is_ok_and(|mut f| f.unregister(&id))
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
    let format = input_format(format.as_deref())?;
    let bytes = doc_bytes(doc);
    let fonts = fonts_snapshot();
    let mut store = ImageStore::new();
    let check_images = images.is_some();
    for (id, b) in images.unwrap_or_default() {
        store.insert(id, b.to_vec());
    }
    let errors = catch_unwind(AssertUnwindSafe(|| {
        inkoxide::validate(&bytes, format, &fonts, &store)
    }))
    .map_err(|_| engine_error("INTERNAL", "", "the engine panicked"))?;
    Ok(errors
        .into_iter()
        .filter(|e| check_images || e.code != inkoxide::ValidationCode::UnknownImage)
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

/// Panics inside `catch_unwind`, exactly like an engine bug would, so the
/// tests can check that a panic surfaces as an `INTERNAL` error instead of
/// taking the process down. Not part of the public API.
#[doc(hidden)]
#[napi(js_name = "__testPanic")]
pub fn test_panic() -> Result<()> {
    catch_unwind(|| -> () { panic!("test panic") })
        .map_err(|_| engine_error("INTERNAL", "", "the engine panicked"))
}
