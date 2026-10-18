//! Render options as JSON: the object JavaScript passed, without its
//! `images`. `packages/wasm/index.js` checks the types first, as the Node
//! package does, so a wrong type here is a bug of the wrapper; it is still
//! `INVALID_ARGUMENT` at its path, without the value. JSON has no NaN or
//! infinities: the wrapper sends them as the strings `"NaN"`,
//! `"Infinity"` and `"-Infinity"`, so that the range checks report them.
//! Keys this does not know are ignored, as napi ignores them.
//! `deferredFonts`, the font ids of `RenderOptions::deferred_fonts`, is
//! this binding's own: the wrapper sends the ids of its `deferredFonts`.

use inkoxide_binding::{EncryptionOptions, Failure, Options, PdfA, PdfUa, PermissionOptions};
use serde_json::{Map, Value};

type Object = Map<String, Value>;

/// The options of a render: those the Node binding has too, and the fonts
/// it may be missing.
#[derive(Default)]
pub(crate) struct Parsed {
    pub options: Options,
    pub deferred_fonts: Vec<String>,
}

pub(crate) fn parse(json: &[u8]) -> Result<Parsed, Failure> {
    if json.is_empty() {
        return Ok(Parsed::default());
    }
    let Ok(Value::Object(o)) = serde_json::from_slice(json) else {
        return Err(Failure::invalid_arg("options", "must be an object"));
    };
    let at = "options";
    let options = Options {
        format: string(&o, at, "format")?,
        compress: boolean(&o, at, "compress")?,
        compression_level: number(&o, at, "compressionLevel")?,
        creation_date: string(&o, at, "creationDate")?,
        threads: number(&o, at, "threads")?,
        subset_fonts: boolean(&o, at, "subsetFonts")?,
        pdf_a: match o.get("pdfA") {
            None => None,
            Some(Value::Bool(b)) => Some(PdfA::On(*b)),
            Some(Value::String(s)) => Some(PdfA::Level(s.clone())),
            Some(_) => {
                return Err(Failure::invalid_arg(
                    "options.pdfA",
                    "must be a boolean or a string",
                ))
            }
        },
        encryption: object(&o, at, "encryption")?
            .map(|e| encryption(e, "options.encryption"))
            .transpose()?,
        tagged: boolean(&o, at, "tagged")?,
        pdf_ua: match o.get("pdfUA") {
            None => None,
            Some(Value::Bool(b)) => Some(PdfUa::On(*b)),
            Some(Value::String(s)) => Some(PdfUa::Part(s.clone())),
            Some(_) => {
                return Err(Failure::invalid_arg(
                    "options.pdfUA",
                    "must be a boolean or a string",
                ))
            }
        },
        flatten_alpha: boolean(&o, at, "flattenAlpha")?,
    };
    let deferred_fonts = match o.get("deferredFonts") {
        None => Vec::new(),
        Some(Value::Array(ids)) => ids
            .iter()
            .map(|id| id.as_str().map(str::to_string))
            .collect::<Option<_>>()
            .ok_or_else(|| wrong(at, "deferredFonts", "an array of font ids"))?,
        Some(_) => return Err(wrong(at, "deferredFonts", "an array of font ids")),
    };
    Ok(Parsed {
        options,
        deferred_fonts,
    })
}

fn encryption(o: &Object, at: &str) -> Result<EncryptionOptions, Failure> {
    let permissions = match object(o, at, "permissions")? {
        None => None,
        Some(p) => {
            let at = &format!("{at}.permissions");
            Some(PermissionOptions {
                printing: string(p, at, "printing")?,
                modifying: boolean(p, at, "modifying")?,
                copying: boolean(p, at, "copying")?,
                annotating: boolean(p, at, "annotating")?,
                filling_forms: boolean(p, at, "fillingForms")?,
                content_accessibility: boolean(p, at, "contentAccessibility")?,
                document_assembly: boolean(p, at, "documentAssembly")?,
            })
        }
    };
    Ok(EncryptionOptions {
        user_password: string(o, at, "userPassword")?,
        owner_password: string(o, at, "ownerPassword")?,
        permissions,
        entropy: bytes(o, at, "entropy")?,
    })
}

/// An array of byte values, as the JS wrapper writes a `Uint8Array`.
fn bytes(o: &Object, at: &str, key: &str) -> Result<Option<Vec<u8>>, Failure> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_u64().and_then(|b| u8::try_from(b).ok()))
            .collect::<Option<_>>()
            .map(Some)
            .ok_or_else(|| wrong(at, key, "bytes")),
        Some(_) => Err(wrong(at, key, "bytes")),
    }
}

fn wrong(at: &str, key: &str, what: &str) -> Failure {
    Failure::invalid_arg(&format!("{at}.{key}"), &format!("must be {what}"))
}

fn string(o: &Object, at: &str, key: &str) -> Result<Option<String>, Failure> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(wrong(at, key, "a string")),
    }
}

fn boolean(o: &Object, at: &str, key: &str) -> Result<Option<bool>, Failure> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(wrong(at, key, "a boolean")),
    }
}

fn number(o: &Object, at: &str, key: &str) -> Result<Option<f64>, Failure> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) if s == "NaN" => Ok(Some(f64::NAN)),
        Some(Value::String(s)) if s == "Infinity" => Ok(Some(f64::INFINITY)),
        Some(Value::String(s)) if s == "-Infinity" => Ok(Some(f64::NEG_INFINITY)),
        Some(_) => Err(wrong(at, key, "a number")),
    }
}

fn object<'o>(o: &'o Object, at: &str, key: &str) -> Result<Option<&'o Object>, Failure> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(wrong(at, key, "an object")),
    }
}
