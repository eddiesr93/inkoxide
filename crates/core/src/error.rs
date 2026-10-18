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
    /// `version` is a number other than 1. A missing `version` is
    /// [`MissingField`](Self::MissingField), and one that is not a number
    /// [`InvalidType`](Self::InvalidType).
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
    /// A font id is declared twice, an attachment `name` or an XMP schema
    /// `prefix` is declared twice, or a form field name is used twice
    /// (other than by the buttons of one radio group), or used both for a
    /// field and for a group of fields (`a` and `a.b`).
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
    /// A raster image has more than 50 million pixels (width × height). An
    /// SVG has its own limit, and fails as
    /// [`InvalidImage`](Self::InvalidImage).
    ImageTooLarge = "IMAGE_TOO_LARGE",
    /// An `attachment` node or run names a file that `attachments` does
    /// not declare.
    UnknownAttachment = "UNKNOWN_ATTACHMENT",
    /// An attachment's `src` is not in the render's attachment store.
    /// [`render`](crate::render) and
    /// [`validate_with_options`](crate::validate_with_options) report it:
    /// [`validate`](crate::validate) does not see the store.
    AttachmentNotRegistered = "ATTACHMENT_NOT_REGISTERED",
    /// An attachment is over 64 MiB, or a document's attachments are over
    /// 256 MiB together.
    AttachmentTooLarge = "ATTACHMENT_TOO_LARGE",
    /// A style property was set on a node type it does not apply to.
    StyleNotApplicable = "STYLE_NOT_APPLICABLE",
    /// Table structure is inconsistent (spans, column counts, header rows).
    InvalidTable = "INVALID_TABLE",
    /// A node type is not allowed where it appears.
    InvalidNode = "INVALID_NODE",
    /// A `#id` link names no node in the document.
    UnknownAnchor = "UNKNOWN_ANCHOR",
    /// The document uses something the PDF/A level it is rendered at does
    /// not allow: a page side under 3 points, at any level; under PDF/A-1b,
    /// transparency (a colour with alpha, an `opacity`, `debug` outlines,
    /// an image with an alpha channel, an SVG drawn with opacity, a mask or
    /// a blend mode, an isolated group, a clip path of more than one path,
    /// or a colour glyph that needs transparency); under PDF/A-1b, 2b and
    /// 2a, `attachments`. [`render`](crate::render) reports the first such
    /// use, and so does
    /// [`validate_with_options`](crate::validate_with_options), which knows
    /// the level, but for colour glyphs: a render checks those once the
    /// text is shaped, after the rest, and names one only when nothing else
    /// conflicts. [`validate`](crate::validate) does not know the level.
    PdfaConflict = "PDFA_CONFLICT",
    /// The document lacks something PDF/UA requires (either part): a title, a
    /// language, alternative text on an image or a `figure` box, a label
    /// on a form field, headings that go down one level at a time from
    /// `h1`, or lists made of items (a `listItem` inside a `list`, a
    /// `listBody` inside a `listItem`); under part 2, a `#id` link or a
    /// bookmark that goes to decoration. [`render`](crate::render) reports
    /// it, with [`RenderOptions::pdf_ua`](crate::RenderOptions::pdf_ua),
    /// and so does [`validate_with_options`](crate::validate_with_options).
    UaConflict = "UA_CONFLICT",
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

/// What a render needs and does not have yet ([`InkoxideError::NotLoaded`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct NotLoaded {
    /// Font ids of [`crate::RenderOptions::deferred_fonts`] the render draws
    /// with, or might: register them with the [`crate::FontStore`].
    pub fonts: Vec<String>,
    /// Files of built-in hyphenation patterns ([`crate::builtin_hyphenation_file`]):
    /// register them with [`crate::HyphenationStore::register_builtin`].
    pub hyphenation: Vec<String>,
}

/// Names of what a render found missing, for [`NotLoaded`], noted from any
/// of its threads.
#[derive(Debug, Default)]
pub(crate) struct Missing(std::sync::Mutex<std::collections::BTreeSet<String>>);

impl Missing {
    pub fn note(&self, name: &str) {
        if let Ok(mut names) = self.0.lock() {
            if !names.contains(name) {
                names.insert(name.to_string());
            }
        }
    }

    /// The names noted, in order.
    pub fn names(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|names| names.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl std::fmt::Display for NotLoaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if !self.fonts.is_empty() {
            parts.push(format!("the fonts {}", self.fonts.join(", ")));
        }
        if !self.hyphenation.is_empty() {
            parts.push(format!(
                "the built-in hyphenation patterns {}",
                self.hyphenation.join(", ")
            ));
        }
        write!(f, "not loaded: {}", parts.join(" and "))
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
    /// The render needs what the caller said it would give later: fonts
    /// in [`crate::RenderOptions::deferred_fonts`], or built-in hyphenation
    /// patterns a build without the `embedded-hyphenation` feature has not
    /// been given. Render again once they are registered.
    #[error("{0}")]
    NotLoaded(NotLoaded),
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
            Self::NotLoaded(_) => "NOT_LOADED",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

impl From<ValidationError> for InkoxideError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}
