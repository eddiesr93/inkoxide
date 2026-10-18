//! Display list: the backend-neutral drawing operations of one page.
//!
//! Coordinates are in points with the origin at the **top-left** of the page
//! and y growing downwards (like the layout tree). The PDF sink flips once
//! per page.

mod boxes;
pub(crate) mod debug;
mod shapes;

pub(crate) use boxes::{content_radii, paint_flow, ContentCtx, ContentPainter, FlowPart};
pub(crate) use debug::{draw as draw_debug, page_box as debug_page_box};
pub(crate) use debug::{DebugBoxes, DebugLine, Labels as DebugLabels};
pub(crate) use shapes::{is_square, polygon, rounded_rect, Radii, KAPPA, NO_RADII};

use crate::geom::Rect;
use crate::schema::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathSeg {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Path {
    pub segs: Vec<PathSeg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dash {
    /// On/off lengths.
    pub pattern: [f32; 2],
    pub round_cap: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Save,
    Restore,
    /// Concatenates `[a b c d e f]` to the current transform.
    Transform([f32; 6]),
    ClipRect(Rect),
    ClipPath {
        path: Path,
        rule: FillRule,
    },
    FillRect {
        rect: Rect,
        color: Color,
    },
    FillPath {
        path: Path,
        color: Color,
        rule: FillRule,
    },
    StrokePath {
        path: Path,
        color: Color,
        width: f32,
        dash: Option<Dash>,
    },
    Text(TextOp),
    /// Draws an image `XObject` with its unit square mapped by `matrix`.
    Image {
        key: crate::images::ImageKey,
        matrix: [f32; 6],
    },
    /// Sets the absolute fill and stroke alpha (cumulative opacity is
    /// computed by the painter; PDF `gs` does not multiply).
    SetOpacity(f32),
    /// A clickable area (draws nothing). `rect` is in page coordinates,
    /// under the transform in effect: the writer makes the annotation
    /// cover where it lands.
    Link {
        rect: Rect,
        target: Box<str>,
        /// The text it covers, which says what it is in tagged output
        /// (`None`: nothing to say, or untagged).
        label: Option<Box<str>>,
        /// The structure element of its annotation in tagged output; `None`:
        /// the one whose content is being drawn ([`Op::Tag`]).
        owner: Option<crate::tag::StructId>,
    },
    /// A form field widget (draws nothing here; the writer adds an
    /// annotation with its appearance, transformed like the content
    /// around it).
    Field(Box<crate::forms::FieldOp>),
    /// An icon annotation, such as a text note (draws nothing here; the
    /// writer adds an annotation with its icon where the transform in
    /// effect puts its corner).
    Icon(Box<crate::icons::IconOp>),
    /// Starts a span whose text, when the page is copied, searched or read
    /// aloud, is `actual_text` instead of what the span draws (`/Span` with
    /// `/ActualText`). Closed by [`Op::EndSpan`]; spans pair up like
    /// `Save` and `Restore` and enclose whole save levels. See [`span`].
    Span {
        actual_text: Box<str>,
    },
    EndSpan,
    /// One glyph drawn invisibly, stretched to a width: text that can be
    /// selected and extracted where nothing shows it (see [`span`]).
    InvisibleText(InvisibleGlyph),
    /// A colour glyph, drawn as a glyph of a Type 3 font
    /// (`ColorGlyphs::push`).
    ColorGlyph(ColorGlyphOp),
    /// In tagged output, starts the content of a structure element: what
    /// is drawn until [`Op::EndTag`] belongs to it. Tags do not nest; the
    /// writer ends their marked content at each save level and starts it
    /// again after. Without tagging none is pushed.
    Tag(crate::tag::StructId),
    /// In tagged output, where figure `owner` is on the page (page
    /// coordinates, under the transform in effect): its bounding box.
    /// Draws nothing.
    FigureArea {
        owner: crate::tag::StructId,
        rect: Rect,
    },
    EndTag,
    /// Inside an element's content, starts decoration: what is drawn until
    /// [`Op::EndDecoration`] is an artifact, and the element's content goes
    /// on after it (an image in text with an empty `alt`). It encloses
    /// whole save levels and does not nest. Output without tagging ignores
    /// it.
    Decoration,
    EndDecoration,
    /// Starts (`Some`) or ends a page's header or footer: the artifacts
    /// drawn in it say so. Output without tagging ignores it.
    Band(Option<PageBand>),
}

/// A band of a page, repeated on every page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageBand {
    Header,
    Footer,
}

/// Code `code` of Type 3 font `font` (an index into
/// `ColorGlyphs::type3`) at `size`, its origin at (`x`, `y`) on the
/// baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColorGlyphOp {
    pub font: u16,
    pub code: u8,
    pub size: f32,
    pub x: f32,
    pub y: f32,
}

/// A glyph of `font` at `size`, drawn invisibly (text rendering mode 3)
/// with its baseline origin at (`x`, `y`) and stretched to `width` points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InvisibleGlyph {
    pub font: crate::fonts::FontKey,
    pub size: f32,
    pub x: f32,
    pub y: f32,
    pub glyph: u16,
    pub width: f32,
}

impl InvisibleGlyph {
    /// The space of `font` at `size`, `width` wide on the baseline at
    /// (`x`, `y`); `None` when the font has no space with an advance, or
    /// no outlines (it is never embedded).
    pub fn space(
        fonts: &crate::fonts::FontSet<'_>,
        font: crate::fonts::FontKey,
        size: f32,
        (x, y): (f32, f32),
        width: f32,
    ) -> Option<Self> {
        let f = fonts.get(font);
        if !f.has_outlines() {
            return None;
        }
        let glyph = f.glyph(' ').filter(|&g| g != 0 && f.advance(g) > 0.0)?;
        Some(Self {
            font,
            size,
            x,
            y,
            glyph,
            width,
        })
    }

    /// The font of the invisible space in the span of an image:
    /// `preferred` (the text's own font), or the first font of the
    /// document with outlines and a space, preferring one without colour
    /// glyphs.
    pub fn space_font(
        fonts: &crate::fonts::FontSet<'_>,
        preferred: Option<crate::fonts::FontKey>,
    ) -> Option<crate::fonts::FontKey> {
        let spaced = |k| Self::space(fonts, k, 1.0, (0.0, 0.0), 1.0).is_some();
        let all = || 0..fonts.fonts.len() as crate::fonts::FontKey;
        preferred
            .filter(|&k| spaced(k))
            .or_else(|| all().find(|&k| !fonts.get(k).has_color() && spaced(k)))
            .or_else(|| all().find(|&k| spaced(k)))
    }
}

/// Where the invisible space of an image's span goes: in `font` (or
/// another, [`InvisibleGlyph::space_font`]) at `size`, `width` wide on the
/// baseline at `at`.
#[derive(Clone, Copy)]
pub(crate) struct SpaceAt {
    pub font: Option<crate::fonts::FontKey>,
    pub size: f32,
    pub at: (f32, f32),
    pub width: f32,
}

/// Pushes what `draw` draws; with a non-empty `alt`, inside a span with
/// that text and the invisible space `space` describes.
pub(crate) fn alt_span(
    ops: &mut Vec<Op>,
    fonts: &crate::fonts::FontSet<'_>,
    alt: Option<&str>,
    space: SpaceAt,
    draw: impl FnOnce(&mut Vec<Op>),
) {
    match alt.filter(|a| !a.is_empty()) {
        Some(alt) => {
            let glyph = InvisibleGlyph::space_font(fonts, space.font)
                .and_then(|k| InvisibleGlyph::space(fonts, k, space.size, space.at, space.width));
            span(ops, alt, glyph, draw);
        }
        None => draw(ops),
    }
}

/// Pushes what `draw` draws inside a span whose text is `actual_text`
/// (for an image, its alternative text). With `text`, the span also holds
/// that invisible glyph, laid over the drawing: some extractors read
/// `ActualText` only off a span that contains text.
pub(crate) fn span(
    ops: &mut Vec<Op>,
    actual_text: &str,
    text: Option<InvisibleGlyph>,
    draw: impl FnOnce(&mut Vec<Op>),
) {
    ops.push(Op::Span {
        actual_text: actual_text.into(),
    });
    draw(ops);
    ops.extend(text.map(Op::InvisibleText));
    ops.push(Op::EndSpan);
}

/// Runs `f`, which draws in a colour of alpha `alpha`, with that alpha
/// folded into `opacity`, the opacity in effect.
pub(crate) fn with_alpha(
    ops: &mut Vec<Op>,
    opacity: f32,
    alpha: f32,
    f: impl FnOnce(&mut Vec<Op>),
) {
    if alpha >= 1.0 {
        f(ops);
        return;
    }
    ops.push(Op::Save);
    ops.push(Op::SetOpacity(opacity * alpha));
    f(ops);
    ops.push(Op::Restore);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextGlyph {
    pub id: u16,
    /// Pen position relative to the op's origin, in points.
    pub x: f32,
    /// Vertical offset, points, positive up.
    pub y_offset: f32,
    /// Whether it follows the first glyph of its cluster (a mark the font
    /// draws apart), which holds the cluster's text: it has none of its own.
    pub continues: bool,
}

/// A run of glyphs in one font, size and colour on one baseline.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextOp {
    pub font: crate::fonts::FontKey,
    pub size: f32,
    /// RGB; alpha is applied through `SetOpacity`.
    pub color: Color,
    /// Baseline origin (layout space).
    pub x: f32,
    pub y: f32,
    pub glyphs: Vec<TextGlyph>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PageContent {
    pub width: f32,
    pub height: f32,
    pub ops: Vec<Op>,
}

impl PageContent {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ops: Vec::new(),
        }
    }
}
