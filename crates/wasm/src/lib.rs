//! WebAssembly binding for Inkoxide (npm `@inkoxide/wasm`). The contract
//! of the Node binding (`crates/node`), through a small C ABI that
//! `packages/wasm/index.js` calls; the module imports nothing.
//!
//! - **Input.** Each byte buffer is allocated with [`ink_alloc`], filled
//!   by JavaScript and passed as a pointer and a length. The call takes it
//!   and frees it.
//! - **Output.** A call that has something to return leaves it as the
//!   result: [`ink_result_ptr`] and [`ink_result_len`] locate it until the
//!   next call, and [`ink_result_free`] drops it. It returns 0 when the
//!   result is its value, and 1 when the result is an error, as
//!   `{ code, path, message }` JSON. A render returns 2 when it needs what
//!   it has not been given: fonts of its `deferredFonts`, or built-in
//!   hyphenation patterns, which the module does not embed, to keep it
//!   small. The result is then `{ fonts, hyphenation }` JSON: JavaScript
//!   registers them ([`ink_register_font`],
//!   [`ink_register_builtin_hyphenation`]) and renders again.
//! - **Options** come as JSON (`options.rs`), images as one buffer
//!   (`images.rs`).
//!
//! A panic aborts the instance: the JavaScript side starts a new one.
//! Every operation is a safe function of byte slices, tested natively; the
//! `ink_*` exports only move the bytes.

// Every error is a `Failure` for JavaScript (see `inkoxide-binding`).
#![allow(clippy::missing_errors_doc)]

mod images;
mod options;

use std::cell::RefCell;

use inkoxide::{FontStore, HyphenationStore, InkoxideError, InputFormat};
use inkoxide_binding::Failure;

/// Threads a render may ask for: WebAssembly here has none to start.
const MAX_THREADS: u32 = 1;

thread_local! {
    /// Fonts registered through `registerFont`, shared by all renders.
    static FONTS: RefCell<FontStore> = RefCell::default();
    /// Patterns registered through `registerHyphenationPatterns`.
    static HYPHENATION: RefCell<HyphenationStore> = RefCell::default();
    /// The result of the last call.
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn text(bytes: &[u8], path: &str) -> Result<String, Failure> {
    String::from_utf8(bytes.to_vec()).map_err(|_| Failure::invalid_arg(path, "must be valid UTF-8"))
}

pub fn register_font(id: &[u8], data: &[u8]) -> Result<(), Failure> {
    let id = text(id, "")?;
    FONTS.with(|f| f.borrow_mut().register(id, data.to_vec()))?;
    Ok(())
}

pub fn unregister_font(id: &[u8]) -> bool {
    std::str::from_utf8(id).is_ok_and(|id| FONTS.with(|f| f.borrow_mut().unregister(id)))
}

pub fn register_hyphenation_patterns(lang: &[u8], data: &[u8]) -> Result<(), Failure> {
    let lang = text(lang, "")?;
    HYPHENATION.with(|h| h.borrow_mut().register(&lang, data))?;
    Ok(())
}

pub fn unregister_hyphenation_patterns(lang: &[u8]) -> bool {
    std::str::from_utf8(lang).is_ok_and(|l| HYPHENATION.with(|h| h.borrow_mut().unregister(l)))
}

/// Validation errors as JSON; `images` checks image references too.
pub fn validate(doc: &[u8], format: &[u8], images: Option<&[u8]>) -> Result<String, Failure> {
    let format = match format {
        [] => None,
        f => Some(text(f, "options.format")?),
    };
    let format = inkoxide_binding::input_format(format.as_deref(), "options.format")?;
    if format == InputFormat::MessagePack {
        let e = Failure::new("PARSE_ERROR", "", inkoxide::JSON_ONLY);
        return Ok(format!("[{}]", e.to_json()));
    }
    let images = images.map(images::parse).transpose()?;
    let errors = FONTS.with(|f| {
        let fonts = f.borrow();
        match &images {
            Some(images) => inkoxide::validate(doc, format, &fonts, images),
            None => inkoxide::validate_without_images(doc, format, &fonts),
        }
    });
    Ok(inkoxide_binding::validation_json(&errors))
}

/// Why a render did not give a PDF.
#[derive(Debug)]
pub enum NotRendered {
    Failed(Failure),
    /// What it needs and has not been given.
    Needs(inkoxide::NotLoaded),
}

impl From<Failure> for NotRendered {
    fn from(f: Failure) -> Self {
        Self::Failed(f)
    }
}

pub fn register_builtin_hyphenation(file: &[u8], automaton: &[u8]) -> Result<(), Failure> {
    let file = text(file, "")?;
    HYPHENATION.with(|h| h.borrow_mut().register_builtin(&file, automaton))?;
    Ok(())
}

/// The file of the built-in patterns for text in `lang`, or empty.
pub fn hyphenation_file(lang: &[u8]) -> String {
    std::str::from_utf8(lang)
        .ok()
        .and_then(inkoxide::builtin_hyphenation_file)
        .unwrap_or_default()
        .to_string()
}

/// The PDF of `doc`, with `options` as JSON, and `images` and `files` (the
/// attachments) as one buffer each.
pub fn render(
    doc: &[u8],
    options: &[u8],
    images: &[u8],
    files: &[u8],
) -> Result<Vec<u8>, NotRendered> {
    let options::Parsed {
        options,
        deferred_fonts,
    } = options::parse(options)?;
    let images = images::parse(images)?;
    let hyphenation = HYPHENATION.with(|h| h.borrow().clone());
    let (format, core) = options.into_core(hyphenation, MAX_THREADS)?;
    let core = core
        .with_deferred_fonts(deferred_fonts)
        .with_attachments(images::parse_files(files)?);
    // The engine is built without `msgpack` for this binding, but a
    // workspace build unifies features: refuse it here too.
    if format == InputFormat::MessagePack {
        return Err(Failure::new("PARSE_ERROR", "", inkoxide::JSON_ONLY).into());
    }
    match FONTS.with(|f| inkoxide::render(doc, format, &core, &f.borrow(), &images)) {
        Ok(pdf) => Ok(pdf),
        Err(InkoxideError::NotLoaded(missing)) => Err(NotRendered::Needs(missing)),
        Err(e) => Err(NotRendered::Failed(e.into())),
    }
}

/// `{ engine, schema, gitSha }` as JSON.
pub fn version() -> String {
    format!(
        "{{\"engine\":{},\"schema\":{},\"gitSha\":{}}}",
        inkoxide_binding::json_string(inkoxide::ENGINE_VERSION),
        inkoxide::SCHEMA_VERSION,
        inkoxide_binding::json_string(env!("INKOXIDE_GIT_SHA"))
    )
}

// The C ABI.

/// Takes a buffer [`ink_alloc`] gave.
///
/// # Safety
///
/// `ptr` and `len` are a pointer [`ink_alloc`] returned and the length it
/// was given, and the buffer is not used again.
unsafe fn take(ptr: *mut u8, len: usize) -> Box<[u8]> {
    // SAFETY: `ink_alloc` leaked a `Box<[u8]>` of `len` bytes at `ptr`.
    unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) }
}

/// Sets the result, and returns 0 for a value or 1 for an error.
fn finish(result: Result<Vec<u8>, Failure>) -> u32 {
    let (bytes, status) = match result {
        Ok(v) => (v, 0),
        Err(f) => (f.to_json().into_bytes(), 1),
    };
    RESULT.with(|r| *r.borrow_mut() = bytes);
    status
}

/// A zeroed buffer of `len` bytes for an input, which the call it is
/// passed to frees.
#[unsafe(no_mangle)]
pub extern "C" fn ink_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()).cast()
}

/// Frees a buffer [`ink_alloc`] gave that was not passed to a call.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_free(ptr: *mut u8, len: usize) {
    drop(unsafe { take(ptr, len) });
}

/// Where the last result starts.
#[unsafe(no_mangle)]
pub extern "C" fn ink_result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}

/// How long the last result is.
#[unsafe(no_mangle)]
pub extern "C" fn ink_result_len() -> usize {
    RESULT.with(|r| r.borrow().len())
}

/// Drops the last result.
#[unsafe(no_mangle)]
pub extern "C" fn ink_result_free() {
    RESULT.with(|r| *r.borrow_mut() = Vec::new());
}

/// [`register_font`]; the result is empty or an error.
///
/// # Safety
///
/// Each pointer and length is as [`take`] says.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_register_font(
    id: *mut u8,
    id_len: usize,
    data: *mut u8,
    data_len: usize,
) -> u32 {
    let (id, data) = unsafe { (take(id, id_len), take(data, data_len)) };
    finish(register_font(&id, &data).map(|()| Vec::new()))
}

/// [`unregister_font`]: 1 when the font was registered.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_unregister_font(id: *mut u8, id_len: usize) -> u32 {
    let id = unsafe { take(id, id_len) };
    u32::from(unregister_font(&id))
}

/// [`register_hyphenation_patterns`]; the result is empty or an error.
///
/// # Safety
///
/// Each pointer and length is as [`take`] says.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_register_hyphenation_patterns(
    lang: *mut u8,
    lang_len: usize,
    data: *mut u8,
    data_len: usize,
) -> u32 {
    let (lang, data) = unsafe { (take(lang, lang_len), take(data, data_len)) };
    finish(register_hyphenation_patterns(&lang, &data).map(|()| Vec::new()))
}

/// [`unregister_hyphenation_patterns`]: 1 when there were patterns.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_unregister_hyphenation_patterns(
    lang: *mut u8,
    lang_len: usize,
) -> u32 {
    let lang = unsafe { take(lang, lang_len) };
    u32::from(unregister_hyphenation_patterns(&lang))
}

/// [`validate`]; `format` is empty for JSON, and `has_images` 0 leaves
/// the image references unchecked (`images` is then empty).
///
/// # Safety
///
/// Each pointer and length is as [`take`] says.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_validate(
    doc: *mut u8,
    doc_len: usize,
    format: *mut u8,
    format_len: usize,
    images: *mut u8,
    images_len: usize,
    has_images: u32,
) -> u32 {
    let (doc, format, images) = unsafe {
        (
            take(doc, doc_len),
            take(format, format_len),
            take(images, images_len),
        )
    };
    let images = (has_images != 0).then_some(&images[..]);
    finish(validate(&doc, &format, images).map(String::into_bytes))
}

/// [`render`]: the result is the PDF or an error.
///
/// # Safety
///
/// Each pointer and length is as [`take`] says.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_render(
    doc: *mut u8,
    doc_len: usize,
    options: *mut u8,
    options_len: usize,
    images: *mut u8,
    images_len: usize,
    files: *mut u8,
    files_len: usize,
) -> u32 {
    let (doc, options, images, files) = unsafe {
        (
            take(doc, doc_len),
            take(options, options_len),
            take(images, images_len),
            take(files, files_len),
        )
    };
    match render(&doc, &options, &images, &files) {
        Ok(pdf) => finish(Ok(pdf)),
        Err(NotRendered::Failed(f)) => finish(Err(f)),
        Err(NotRendered::Needs(missing)) => {
            let list = |ids: &[String]| {
                let quoted: Vec<String> = ids
                    .iter()
                    .map(|id| inkoxide_binding::json_string(id))
                    .collect();
                format!("[{}]", quoted.join(","))
            };
            let json = format!(
                "{{\"fonts\":{},\"hyphenation\":{}}}",
                list(&missing.fonts),
                list(&missing.hyphenation)
            );
            finish(Ok(json.into_bytes()));
            2
        }
    }
}

/// [`register_builtin_hyphenation`]; the result is empty or an error.
///
/// # Safety
///
/// Each pointer and length is as [`take`] says.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_register_builtin_hyphenation(
    file: *mut u8,
    file_len: usize,
    data: *mut u8,
    data_len: usize,
) -> u32 {
    let (file, data) = unsafe { (take(file, file_len), take(data, data_len)) };
    finish(register_builtin_hyphenation(&file, &data).map(|()| Vec::new()))
}

/// [`hyphenation_file`] as the result.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ink_hyphenation_file(lang: *mut u8, lang_len: usize) -> u32 {
    let lang = unsafe { take(lang, lang_len) };
    finish(Ok(hyphenation_file(&lang).into_bytes()))
}

/// [`version`] as the result.
#[unsafe(no_mangle)]
pub extern "C" fn ink_version() -> u32 {
    finish(Ok(version().into_bytes()))
}

/// [`inkoxide::clear_caches`].
#[unsafe(no_mangle)]
pub extern "C" fn ink_clear_caches() {
    inkoxide::clear_caches();
}

/// Panics, as an engine bug would, so that the tests can check that the
/// JavaScript side recovers from the trap. Not part of the public API.
#[doc(hidden)]
#[unsafe(no_mangle)]
pub extern "C" fn ink_test_panic() {
    panic!("test panic");
}

#[cfg(test)]
mod tests;
