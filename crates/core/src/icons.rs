//! Icon annotations: annotations a viewer shows as an icon of a fixed
//! size at a point, which painting hands to the PDF writer: text notes and
//! file attachments.

use crate::schema::{Attachment, Note};

/// Side of an icon, points. Viewers draw it this size at any zoom.
pub(crate) const ICON: f32 = 20.0;

/// An icon annotation at a point. Its appearance is drawn by the PDF
/// writer, upright and [`ICON`] square whatever the transform: only its
/// corner moves with it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IconOp {
    /// The icon's top-left corner, in page coordinates (top-left origin),
    /// under the transform in effect.
    pub x: f32,
    pub y: f32,
    pub kind: IconKind,
}

/// What an icon annotation holds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IconKind {
    /// A text note (`/Subtype /Text`).
    Note(Note),
    /// A file attachment (`/Subtype /FileAttachment`).
    Attachment(Attachment),
}
