//! Text annotations: the icon op painting hands to the PDF writer.

use crate::layout::Geometry;
use crate::paint::{ContentCtx, Op};
use crate::schema::Note;

/// Side of a note's icon, points. Viewers draw it this size at any zoom.
pub(crate) const ICON: f32 = 20.0;

/// An annotation shown as an icon of a fixed size at a point: a text
/// note. Its appearance is drawn by the PDF writer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoteOp {
    /// The icon's top-left corner, in page coordinates (top-left origin),
    /// under the transform in effect.
    pub x: f32,
    pub y: f32,
    pub note: Note,
}

/// Emits the note op of `note` at the top-left corner of the content box
/// of `geo`, on the page where it starts.
pub(crate) fn paint(note: &Note, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
    let b = geo.content_box();
    let top = b.y + ctx.shift;
    if ctx.window.is_some_and(|w| !w.holds(top)) {
        return;
    }
    ops.push(Op::Note(Box::new(NoteOp {
        x: b.x + ctx.dx,
        y: top + ctx.dy,
        note: note.clone(),
    })));
}
