//! What the Node binding (`crates/node`) and the WebAssembly binding
//! (`crates/wasm`) share: the render options as JavaScript passes them,
//! their checks, and errors as `{ code, path, message }` JSON, which the
//! JavaScript side of each binding turns into an `InkoxideError`. Each
//! binding converts its own JavaScript values into [`Options`].

// Every error is a `Failure` for JavaScript, whose message says what is
// wrong where it is made.
#![allow(clippy::missing_errors_doc)]

use std::fmt::Write as _;

use inkoxide::{
    Encryption, HyphenationStore, InkoxideError, InputFormat, PdfALevel, PdfUaPart, Permissions,
    Printing,
};

/// A failure to report to JavaScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub path: String,
    pub message: String,
}

impl Failure {
    pub fn new(code: &str, path: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            path: path.into(),
            message: message.into(),
        }
    }

    /// A JavaScript argument of the wrong value, at `path`.
    pub fn invalid_arg(path: &str, message: &str) -> Self {
        Self::new("INVALID_ARGUMENT", path, message)
    }

    /// A bug inside the engine.
    pub fn internal(message: &str) -> Self {
        Self::new("INTERNAL", "", message)
    }

    /// `{"code":…,"path":…,"message":…}`.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"code\":{},\"path\":{},\"message\":{}}}",
            json_string(&self.code),
            json_string(&self.path),
            json_string(&self.message)
        )
    }
}

impl From<InkoxideError> for Failure {
    fn from(e: InkoxideError) -> Self {
        match &e {
            // A bad option (`creationDate`) is the caller's argument, not
            // the document.
            InkoxideError::Validation(v) if v.path.starts_with("options.") => {
                Self::invalid_arg(&v.path, &v.message)
            }
            InkoxideError::Validation(v) => Self::new(v.code.as_str(), &v.path, &v.message),
            other => Self::new(other.code(), "", &other.to_string()),
        }
    }
}

/// `s` as a JSON string.
pub fn json_string(s: &str) -> String {
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

/// Validation errors as a JSON array of `{ code, path, message }`.
pub fn validation_json(errors: &[inkoxide::ValidationError]) -> String {
    let mut out = String::from("[");
    for (k, e) in errors.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        out.push_str(&Failure::new(e.code.as_str(), &e.path, &e.message).to_json());
    }
    out.push(']');
    out
}

/// A JavaScript number that must be an integer within `min..=max` (NaN,
/// fractions and infinities are not).
pub fn integer(path: &str, v: f64, min: u32, max: u32) -> Result<u32, Failure> {
    if v.fract() == 0.0 && (f64::from(min)..=f64::from(max)).contains(&v) {
        Ok(v as u32)
    } else {
        Err(Failure::invalid_arg(
            path,
            &format!("must be an integer from {min} to {max}, found {v}"),
        ))
    }
}

/// `options.format`, or `validate`'s `format`.
pub fn input_format(format: Option<&str>, path: &str) -> Result<InputFormat, Failure> {
    match format {
        None | Some("json") => Ok(InputFormat::Json),
        Some("msgpack") => Ok(InputFormat::MessagePack),
        Some(other) => Err(Failure::invalid_arg(
            path,
            &format!("unsupported format `{other}` (expected \"json\" or \"msgpack\")"),
        )),
    }
}

fn pdf_a_level(name: &str) -> Result<PdfALevel, Failure> {
    name.parse::<PdfALevel>().map_err(|e| {
        Failure::invalid_arg(
            "options.pdfA",
            &format!("{e}; pdfA also takes true or false"),
        )
    })
}

/// `options.pdfA`: `true`, `false` or a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfA {
    On(bool),
    Level(String),
}

/// `options.pdfUA`: `true` (PDF/UA-1), `false` or a part, `"1"` or `"2"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfUa {
    On(bool),
    Part(String),
}

/// The render options JavaScript passes, except the images, which each
/// binding reads its own way. Numbers are JavaScript numbers, checked
/// here, not converted by the binding.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub format: Option<String>,
    pub compress: Option<bool>,
    pub compression_level: Option<f64>,
    pub creation_date: Option<String>,
    pub threads: Option<f64>,
    pub subset_fonts: Option<bool>,
    pub pdf_a: Option<PdfA>,
    pub encryption: Option<EncryptionOptions>,
    pub tagged: Option<bool>,
    pub pdf_ua: Option<PdfUa>,
    pub flatten_alpha: Option<bool>,
}

/// `options.encryption`: mirrors Rust `Encryption`.
#[derive(Debug, Clone, Default)]
pub struct EncryptionOptions {
    pub user_password: Option<String>,
    pub owner_password: Option<String>,
    pub permissions: Option<PermissionOptions>,
    /// Random bytes mixed into the keys.
    pub entropy: Option<Vec<u8>>,
}

/// `options.encryption.permissions`: mirrors Rust `Permissions`; a field
/// left out keeps its default (all off, `contentAccessibility` on).
#[derive(Debug, Clone, Default)]
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

impl Options {
    /// The input format and engine options these give, with `hyphenation`
    /// and at most `max_threads` threads.
    pub fn into_core(
        self,
        hyphenation: HyphenationStore,
        max_threads: u32,
    ) -> Result<(InputFormat, inkoxide::RenderOptions), Failure> {
        let mut core = inkoxide::RenderOptions::default().with_hyphenation(hyphenation);
        let format = input_format(self.format.as_deref(), "options.format")?;
        if let Some(c) = self.compress {
            core.compress = c;
        }
        if let Some(level) = self.compression_level {
            core.compression_level = integer("options.compressionLevel", level, 0, 9)?;
        }
        core.creation_date = self.creation_date;
        if let Some(t) = self.threads {
            core.threads = Some(integer("options.threads", t, 1, max_threads)? as usize);
        }
        if let Some(s) = self.subset_fonts {
            core.subset_fonts = s;
        }
        match self.pdf_a {
            None => {}
            Some(PdfA::On(a)) => core.pdf_a = a,
            Some(PdfA::Level(level)) => core = core.with_pdf_a_level(pdf_a_level(&level)?),
        }
        if let Some(e) = self.encryption {
            core = core.with_encryption(encryption(e)?);
        }
        core.tagged = self.tagged.unwrap_or(false);
        match self.pdf_ua {
            None | Some(PdfUa::On(false)) => {}
            Some(PdfUa::On(true)) => core.pdf_ua = true,
            Some(PdfUa::Part(part)) => {
                let part = part.parse::<PdfUaPart>().map_err(|e| {
                    Failure::invalid_arg(
                        "options.pdfUA",
                        &format!("{e}; pdfUA also takes true or false"),
                    )
                })?;
                core = core.with_pdf_ua_part(part);
            }
        }
        core.flatten_alpha = self.flatten_alpha.unwrap_or(false);
        Ok((format, core))
    }
}

fn encryption(o: EncryptionOptions) -> Result<Encryption, Failure> {
    let mut e = Encryption::new();
    e.user_password = o.user_password;
    e.owner_password = o.owner_password;
    e.entropy = o.entropy;
    if let Some(p) = o.permissions {
        let mut perms = Permissions::new().with_printing(match p.printing.as_deref() {
            None => None,
            Some("lowResolution") => Some(Printing::LowResolution),
            Some("highResolution") => Some(Printing::HighResolution),
            Some(other) => {
                return Err(Failure::invalid_arg(
                    "options.encryption.permissions.printing",
                    &format!(
                        "unsupported printing `{other}` (expected \"lowResolution\" or \"highResolution\")"
                    ),
                ))
            }
        });
        for (set, value) in [
            (&mut perms.modifying, p.modifying),
            (&mut perms.copying, p.copying),
            (&mut perms.annotating, p.annotating),
            (&mut perms.filling_forms, p.filling_forms),
            (&mut perms.content_accessibility, p.content_accessibility),
            (&mut perms.document_assembly, p.document_assembly),
        ] {
            if let Some(v) = value {
                *set = v;
            }
        }
        e = e.with_permissions(perms);
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_json() {
        let f = Failure::new("PARSE_ERROR", "a.b", "say \"hi\"\\\n");
        assert_eq!(
            f.to_json(),
            r#"{"code":"PARSE_ERROR","path":"a.b","message":"say \"hi\"\\\u000a"}"#
        );
    }

    #[test]
    fn numbers_are_integers_in_range() {
        assert_eq!(integer("x", 4.0, 1, 256), Ok(4));
        for bad in [0.0, 1.5, 257.0, f64::NAN, f64::INFINITY] {
            let e = integer("options.threads", bad, 1, 256).unwrap_err();
            assert_eq!(
                (e.code.as_str(), e.path.as_str()),
                ("INVALID_ARGUMENT", "options.threads")
            );
        }
    }

    #[test]
    fn options_are_checked_at_their_path() {
        let path = |o: Options, max| {
            o.into_core(HyphenationStore::default(), max)
                .unwrap_err()
                .path
        };
        let threads = Options {
            threads: Some(2.0),
            ..Options::default()
        };
        assert!(threads
            .clone()
            .into_core(HyphenationStore::default(), 256)
            .is_ok());
        assert_eq!(path(threads, 1), "options.threads");
        let level = Options {
            pdf_a: Some(PdfA::Level("4z".into())),
            ..Options::default()
        };
        assert_eq!(path(level, 256), "options.pdfA");
        let part = Options {
            pdf_ua: Some(PdfUa::Part("3".into())),
            ..Options::default()
        };
        assert_eq!(path(part, 256), "options.pdfUA");
        let (_, two) = Options {
            pdf_ua: Some(PdfUa::Part("2".into())),
            ..Options::default()
        }
        .into_core(HyphenationStore::default(), 256)
        .unwrap();
        assert!(two.pdf_ua && two.pdf_ua_part == PdfUaPart::Ua2);
        let printing = Options {
            encryption: Some(EncryptionOptions {
                user_password: Some("u".into()),
                permissions: Some(PermissionOptions {
                    printing: Some("always".into()),
                    ..PermissionOptions::default()
                }),
                ..EncryptionOptions::default()
            }),
            ..Options::default()
        };
        assert_eq!(
            path(printing, 256),
            "options.encryption.permissions.printing"
        );
    }
}
