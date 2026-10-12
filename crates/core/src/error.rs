use std::fmt;

/// Declares [`ValidationCode`] from one list of variants and their string
/// forms, so `as_str` and `from_str` cannot drift apart.
macro_rules! validation_codes {
    ($($(#[$m:meta])* $variant:ident = $s:literal,)+) => {
        /// Machine-readable validation error codes. The string form (e.g.
        /// `UNKNOWN_FONT`) is part of the public contract.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum ValidationCode {
            $($(#[$m])* $variant,)+
        }

        impl ValidationCode {
            /// The stable string form, e.g. `UNKNOWN_FONT`.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $s,)+
                }
            }

            pub(crate) fn from_str(s: &str) -> Option<Self> {
                match s {
                    $($s => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

validation_codes! {
    /// The input is not well-formed JSON / `MessagePack`.
    ParseError = "PARSE_ERROR",
    /// `version` is missing or not supported by this engine.
    UnsupportedVersion = "UNSUPPORTED_VERSION",
    /// A property the schema does not define.
    UnknownField = "UNKNOWN_FIELD",
    /// A required property is absent.
    MissingField = "MISSING_FIELD",
    /// A value has the wrong JSON type.
    InvalidType = "INVALID_TYPE",
    /// A value has the right type but is not accepted (an unknown enum
    /// string, a malformed colour, a misplaced pagination control).
    InvalidValue = "INVALID_VALUE",
    /// A number is outside its allowed range, or page margins leave no room
    /// for content.
    OutOfRange = "OUT_OF_RANGE",
    /// A font id is declared twice, or a form field name is used twice
    /// (other than by the buttons of one radio group).
    DuplicateId = "DUPLICATE_ID",
    /// A run or `fallbackFonts` references a font id not declared in `fonts`.
    UnknownFont = "UNKNOWN_FONT",
    /// A declared font has no bytes registered with the engine.
    FontNotRegistered = "FONT_NOT_REGISTERED",
    /// An image `src` is not present in the render-time image map.
    UnknownImage = "UNKNOWN_IMAGE",
    /// Image bytes are not a supported JPEG, PNG, GIF, WebP or SVG, or do
    /// not decode. A JPEG is not decoded: its structure is checked.
    InvalidImage = "INVALID_IMAGE",
    /// An image has more than 50 million pixels (width × height).
    ImageTooLarge = "IMAGE_TOO_LARGE",
    /// A style property was set on a node type it does not apply to.
    StyleNotApplicable = "STYLE_NOT_APPLICABLE",
    /// Table structure is inconsistent (spans, column counts, header rows).
    InvalidTable = "INVALID_TABLE",
    /// A node type is not allowed where it appears.
    InvalidNode = "INVALID_NODE",
    /// A `#id` link names no node in the document.
    UnknownAnchor = "UNKNOWN_ANCHOR",
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
    /// Use [`crate::validate`] to obtain the others.
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
