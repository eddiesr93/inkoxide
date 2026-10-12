//! Parsing: JSON or `MessagePack` into [`Document`], with serde errors
//! mapped to validation errors that keep their path and code.

use std::fmt::Write as _;

use serde::de::{Deserializer, Visitor};
use serde::Deserialize;

use crate::error::{ValidationCode as Code, ValidationError};
use crate::schema::{Document, SCHEMA_VERSION};
use crate::InputFormat;

pub(crate) fn parse(doc: &[u8], format: InputFormat) -> Result<Document, ValidationError> {
    match format {
        InputFormat::Json => parse_json(doc),
        InputFormat::MessagePack => parse_msgpack(doc),
    }
}

/// Same nesting limit as `serde_json` (rmp-serde allows 1024, enough to
/// overflow the stack of a worker thread).
const MAX_DEPTH: usize = 128;

type MsgpackDe<'a> =
    rmp_serde::Deserializer<rmp_serde::decode::ReadReader<std::io::Cursor<&'a [u8]>>>;

fn msgpack_de(doc: &[u8]) -> MsgpackDe<'_> {
    let mut de = rmp_serde::Deserializer::new(std::io::Cursor::new(doc));
    de.set_max_depth(MAX_DEPTH);
    de
}

fn trailing_bytes() -> ValidationError {
    ValidationError {
        code: Code::ParseError,
        path: String::new(),
        message: "trailing bytes after the MessagePack document".into(),
    }
}

fn parse_msgpack(doc: &[u8]) -> Result<Document, ValidationError> {
    use rmp_serde::decode::Error as E;

    let mut de = msgpack_de(doc);
    if let Ok(d) = Document::deserialize(&mut de) {
        return if de.position() as usize == doc.len() {
            Ok(d)
        } else {
            Err(trailing_bytes())
        };
    }
    let mut de = msgpack_de(doc);
    let err = match serde_path_to_error::deserialize::<_, Document>(&mut de) {
        Ok(d) if de.position() as usize == doc.len() => return Ok(d),
        Ok(_) => return Err(trailing_bytes()),
        Err(e) => e,
    };
    if let Some(e) = version_error(&mut msgpack_de(doc)) {
        return Err(e);
    }
    let path = format_path(err.path());
    match err.into_inner() {
        // Messages from serde itself and from our deserialisers.
        E::Syntax(msg) => Err(classify(path, &msg)),
        // An array with more items than its fixed length, as in JSON.
        E::LengthMismatch(_) if !path.is_empty() => Err(too_many_items(path)),
        // A reserved marker (0xc1) is malformed input, not a type error.
        E::TypeMismatch(marker) if !path.is_empty() && marker.to_u8() != 0xc1 => {
            Err(ValidationError {
                code: Code::InvalidType,
                path,
                message: format!("invalid type: unexpected MessagePack {marker:?}"),
            })
        }
        other => Err(ValidationError {
            code: Code::ParseError,
            path: String::new(),
            message: format!("not valid MessagePack: {other}"),
        }),
    }
}

fn parse_json(doc: &[u8]) -> Result<Document, ValidationError> {
    // Fast path: path tracking costs about a third of the parse, and is
    // only needed to report an error. Parse again with it on failure.
    if let Ok(d) = serde_json::from_slice::<Document>(doc) {
        return Ok(d);
    }
    let mut de = serde_json::Deserializer::from_slice(doc);
    let result: Result<Document, _> = serde_path_to_error::deserialize(&mut de);
    let err = match result {
        Ok(d) => {
            return de.end().map(|()| d).map_err(|e| ValidationError {
                code: Code::ParseError,
                path: String::new(),
                message: e.to_string(),
            })
        }
        Err(e) => e,
    };

    if let Some(e) = version_error(&mut serde_json::Deserializer::from_slice(doc)) {
        return Err(e);
    }

    let path = format_path(err.path());
    let inner = err.into_inner();
    let raw = inner.to_string();
    let msg = match raw.rfind(" at line ") {
        Some(i) if !inner.is_syntax() && !inner.is_eof() => raw[..i].to_string(),
        _ => raw,
    };
    // Inside the document, "trailing characters" is an array with more
    // items than its fixed length (a colour, a transform function):
    // `serde_json` reports it once the items it needed are read.
    if inner.is_syntax() && !path.is_empty() && msg.starts_with("trailing characters") {
        return Err(too_many_items(path));
    }
    if inner.is_syntax() || inner.is_eof() || inner.is_io() {
        return Err(ValidationError {
            code: Code::ParseError,
            path: String::new(),
            message: msg,
        });
    }
    Err(classify(path, &msg))
}

/// A document from another schema version usually fails structurally;
/// report the version, not the symptoms. Reads `version` alone, the same
/// way from either format: a number other than 1 is unsupported. Another
/// type is left to the structural error (`INVALID_TYPE`).
fn version_error<'de, D: Deserializer<'de>>(de: D) -> Option<ValidationError> {
    /// A `version` that is a number; any other type fails the probe.
    struct Number(f64);
    impl<'de> Deserialize<'de> for Number {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl Visitor<'_> for V {
                type Value = Number;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a number")
                }
                fn visit_f64<E>(self, v: f64) -> Result<Number, E> {
                    Ok(Number(v))
                }
                fn visit_i64<E>(self, v: i64) -> Result<Number, E> {
                    Ok(Number(v as f64))
                }
                fn visit_u64<E>(self, v: u64) -> Result<Number, E> {
                    Ok(Number(v as f64))
                }
            }
            d.deserialize_any(V)
        }
    }
    #[derive(Deserialize)]
    struct Probe {
        version: Option<Number>,
    }
    let Number(v) = Probe::deserialize(de).ok()?.version?;
    (v != f64::from(SCHEMA_VERSION)).then(|| unsupported_version(&v.to_string()))
}

fn too_many_items(path: String) -> ValidationError {
    ValidationError {
        code: Code::InvalidValue,
        path,
        message: "the array has more items than its fixed length".into(),
    }
}

pub(super) fn unsupported_version(found: &str) -> ValidationError {
    ValidationError {
        code: Code::UnsupportedVersion,
        path: "version".into(),
        message: format!(
            "document version {found} is not supported; this engine reads version {SCHEMA_VERSION}"
        ),
    }
}

fn format_path(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    let mut out = String::new();
    for seg in path {
        match seg {
            Segment::Seq { index } => {
                let _ = write!(out, "[{index}]");
            }
            Segment::Map { key } => push_key(&mut out, key),
            Segment::Enum { variant } => push_key(&mut out, variant),
            Segment::Unknown => push_key(&mut out, "?"),
        }
    }
    out
}

pub(super) fn push_key(out: &mut String, key: &str) {
    if !out.is_empty() {
        out.push('.');
    }
    out.push_str(key);
}

fn backticked(msg: &str) -> Option<&str> {
    let start = msg.find('`')? + 1;
    let len = msg[start..].find('`')?;
    Some(&msg[start..start + len])
}

/// Maps a serde message to a structured error. Our own deserialisers prefix
/// messages with `[CODE]` or `[CODE@field]`.
fn classify(mut path: String, msg: &str) -> ValidationError {
    if let Some(rest) = msg.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let (tag, message) = (&rest[..end], rest[end + 1..].trim_start());
            let (code, field) = match tag.split_once('@') {
                Some((c, f)) => (c, Some(f)),
                None => (tag, None),
            };
            if let Some(code) = Code::from_str(code) {
                if let Some(f) = field {
                    push_key(&mut path, f);
                }
                return ValidationError {
                    code,
                    path,
                    message: message.to_string(),
                };
            }
        }
    }
    let (code, field) = if msg.starts_with("unknown field") {
        (Code::UnknownField, backticked(msg))
    } else if msg.starts_with("missing field") {
        (Code::MissingField, backticked(msg))
    } else if msg.starts_with("unknown variant")
        || msg.starts_with("invalid value")
        || msg.starts_with("invalid length")
    {
        (Code::InvalidValue, None)
    } else if msg.starts_with("invalid type") || msg.starts_with("data did not match") {
        (Code::InvalidType, None)
    } else {
        (Code::ParseError, None)
    };
    let message = match (code, field) {
        // serde lists every accepted field; for the raw node struct that
        // list is misleading, so keep the message short.
        (Code::UnknownField, Some(f)) => format!("unknown field `{f}`"),
        (Code::MissingField, Some(f)) => format!("missing required field `{f}`"),
        _ => msg.to_string(),
    };
    // serde_path_to_error already ends the path at an unknown key.
    if let (Code::MissingField, Some(f)) = (code, field) {
        push_key(&mut path, f);
    }
    ValidationError {
        code,
        path,
        message,
    }
}
