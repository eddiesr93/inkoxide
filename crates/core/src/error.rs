use std::fmt;

/// Machine-readable validation error codes. The string form (e.g.
/// `UNKNOWN_FONT`) is part of the public contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationCode {
    /// The input is not well-formed JSON / MessagePack.
    ParseError,
    /// `version` is missing or not supported by this engine.
    UnsupportedVersion,
    UnknownField,
    MissingField,
    InvalidType,
    InvalidValue,
    OutOfRange,
    DuplicateId,
    /// A run or `fallbackFonts` references a font id not declared in `fonts`.
    UnknownFont,
    /// A declared font has no bytes registered with the engine.
    FontNotRegistered,
    /// An image `src` is not present in the render-time image map.
    UnknownImage,
    InvalidImage,
    ImageTooLarge,
    /// A style property was set on a node type it does not apply to.
    StyleNotApplicable,
    /// Table structure is inconsistent (spans, column counts, header rows).
    InvalidTable,
    /// A node type is not allowed where it appears.
    InvalidNode,
    /// A `#id` link names no node in the document.
    UnknownAnchor,
}

impl ValidationCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "PARSE_ERROR",
            Self::UnsupportedVersion => "UNSUPPORTED_VERSION",
            Self::UnknownField => "UNKNOWN_FIELD",
            Self::MissingField => "MISSING_FIELD",
            Self::InvalidType => "INVALID_TYPE",
            Self::InvalidValue => "INVALID_VALUE",
            Self::OutOfRange => "OUT_OF_RANGE",
            Self::DuplicateId => "DUPLICATE_ID",
            Self::UnknownFont => "UNKNOWN_FONT",
            Self::FontNotRegistered => "FONT_NOT_REGISTERED",
            Self::UnknownImage => "UNKNOWN_IMAGE",
            Self::InvalidImage => "INVALID_IMAGE",
            Self::ImageTooLarge => "IMAGE_TOO_LARGE",
            Self::StyleNotApplicable => "STYLE_NOT_APPLICABLE",
            Self::InvalidTable => "INVALID_TABLE",
            Self::InvalidNode => "INVALID_NODE",
            Self::UnknownAnchor => "UNKNOWN_ANCHOR",
        }
    }

    pub(crate) fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "PARSE_ERROR" => Self::ParseError,
            "UNSUPPORTED_VERSION" => Self::UnsupportedVersion,
            "UNKNOWN_FIELD" => Self::UnknownField,
            "MISSING_FIELD" => Self::MissingField,
            "INVALID_TYPE" => Self::InvalidType,
            "INVALID_VALUE" => Self::InvalidValue,
            "OUT_OF_RANGE" => Self::OutOfRange,
            "DUPLICATE_ID" => Self::DuplicateId,
            "UNKNOWN_FONT" => Self::UnknownFont,
            "FONT_NOT_REGISTERED" => Self::FontNotRegistered,
            "UNKNOWN_IMAGE" => Self::UnknownImage,
            "INVALID_IMAGE" => Self::InvalidImage,
            "IMAGE_TOO_LARGE" => Self::ImageTooLarge,
            "STYLE_NOT_APPLICABLE" => Self::StyleNotApplicable,
            "INVALID_TABLE" => Self::InvalidTable,
            "INVALID_NODE" => Self::InvalidNode,
            "UNKNOWN_ANCHOR" => Self::UnknownAnchor,
            _ => return None,
        })
    }
}

impl fmt::Display for ValidationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One structured validation problem, e.g.
/// `{ code: UNKNOWN_FONT, path: "pages[0].children[3].runs[1].font", … }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub code: ValidationCode,
    /// JSON-path-like location (`pages[0].children[2].style.width`); empty
    /// for document-level problems.
    pub path: String,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}: {}", self.code, self.message)
        } else {
            write!(f, "{} at {}: {}", self.code, self.path, self.message)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InkoxideError {
    /// The document failed validation; carries the first problem found.
    /// Use [`crate::validate`] to obtain the full list.
    #[error("{}", .0)]
    Validation(ValidationError),
    #[error("font error: {0}")]
    Font(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("layout error: {0}")]
    Layout(String),
    #[error("pdf error: {0}")]
    Pdf(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl InkoxideError {
    /// Stable machine-readable code (`UNKNOWN_FONT`, `FONT_ERROR`, …).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(e) => e.code.as_str(),
            Self::Font(_) => "FONT_ERROR",
            Self::Image(_) => "IMAGE_ERROR",
            Self::Layout(_) => "LAYOUT_ERROR",
            Self::Pdf(_) => "PDF_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

impl From<ValidationError> for InkoxideError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}
