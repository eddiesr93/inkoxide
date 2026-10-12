//! Parsing: JSON or `MessagePack` into [`Document`], with serde errors
//! mapped to validation errors that keep their path and code.

use std::fmt::Write as _;

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
    let Err(err) = serde_path_to_error::deserialize::<_, Document>(&mut de) else {
        return Err(trailing_bytes());
    };
    if let Some(v) = probe_version_msgpack(doc) {
        if v != i64::from(SCHEMA_VERSION) {
            return Err(unsupported_version(&v.to_string()));
        }
    }
    let path = format_path(err.path());
    match err.into_inner() {
        // Messages from serde itself and from our deserialisers.
        E::Syntax(msg) => Err(classify(path, &msg)),
        // A reserved marker (0xc1) is malformed input, not a type error.
        E::TypeMismatch(marker) if !path.is_empty() && format!("{marker:?}") != "Reserved" => {
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

fn probe_version_msgpack(doc: &[u8]) -> Option<i64> {
    #[derive(Deserialize)]
    struct Probe {
        version: Option<i64>,
    }
    rmp_serde::from_slice::<Probe>(doc).ok()?.version
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

    // A document from another schema version usually fails structurally;
    // report the version, not the symptoms.
    if let Some(v) = probe_version(doc) {
        if v != SCHEMA_VERSION {
            return Err(unsupported_version(&v.to_string()));
        }
    }

    let path = format_path(err.path());
    let inner = err.into_inner();
    let raw = inner.to_string();
    let msg = match raw.rfind(" at line ") {
        Some(i) if !inner.is_syntax() && !inner.is_eof() => raw[..i].to_string(),
        _ => raw,
    };
    if inner.is_syntax() || inner.is_eof() || inner.is_io() {
        return Err(ValidationError {
            code: Code::ParseError,
            path: String::new(),
            message: msg,
        });
    }
    Err(classify(path, &msg))
}

fn probe_version(doc: &[u8]) -> Option<serde_json::Value> {
    #[derive(Deserialize)]
    struct Probe {
        version: Option<serde_json::Value>,
    }
    serde_json::from_slice::<Probe>(doc).ok()?.version
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
