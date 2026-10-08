use std::fmt;

/// Machine-readable validation error codes. The string form (e.g.
/// `UNKNOWN_FONT`) is part of the public contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValidationCode {
    /// The input is not well-formed JSON / `MessagePack`.
    ParseError,
    /// `version` is missing or not supported by this engine.
    UnsupportedVersion,
    /// A property the schema does not define.
    UnknownField,
    /// A required property is absent.
    MissingField,
    /// A value has the wrong JSON type.
    InvalidType,
    /// A value has the right type but is not accepted (an unknown enum
    /// string, a malformed colour, a misplaced pagination control).
    InvalidValue,
    /// A number is outside its allowed range.
    OutOfRange,
    /// A font id is declared twice.
    DuplicateId,
    /// A run or `fallbackFonts` references a font id not declared in `fonts`.
    UnknownFont,
    /// A declared font has no bytes registered with the engine.
    FontNotRegistered,
    /// An image `src` is not present in the render-time image map.
    UnknownImage,
    /// Image bytes are not a supported JPEG, PNG, GIF, WebP or SVG.
    InvalidImage,
    /// A decoded image exceeds the size guard.
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
    /// The stable string form, e.g. `UNKNOWN_FONT`.
    #[must_use]
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
    /// What kind of problem this is.
    pub code: ValidationCode,
    /// JSON-path-like location (`pages[0].children[2].style.width`); empty
    /// for document-level problems.
    pub path: String,
    /// Human-readable description.
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

/// Why a render failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InkoxideError {
    /// The document failed validation; carries the first problem found.
    /// Use [`crate::validate`] to obtain the full list.
    #[error("{}", .0)]
    Validation(ValidationError),
    /// Font bytes could not be parsed, shaped or embedded.
    #[error("font error: {0}")]
    Font(String),
    /// An image could not be decoded or embedded.
    #[error("image error: {0}")]
    Image(String),
    /// The layout engine failed.
    #[error("layout error: {0}")]
    Layout(String),
    /// Writing the PDF failed.
    #[error("pdf error: {0}")]
    Pdf(String),
    /// Hyphenation patterns could not be parsed.
    #[error("hyphenation error: {0}")]
    Hyphenation(String),
    /// A bug or resource failure inside the engine (e.g. the worker pool
    /// could not be built).
    #[error("internal error: {0}")]
    Internal(String),
}

impl InkoxideError {
    /// Stable machine-readable code (`UNKNOWN_FONT`, `FONT_ERROR`, …).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(e) => e.code.as_str(),
            Self::Font(_) => "FONT_ERROR",
            Self::Image(_) => "IMAGE_ERROR",
            Self::Layout(_) => "LAYOUT_ERROR",
            Self::Pdf(_) => "PDF_ERROR",
            Self::Hyphenation(_) => "HYPHENATION_ERROR",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

impl From<ValidationError> for InkoxideError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}
