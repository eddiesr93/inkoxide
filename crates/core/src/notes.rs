//! Icons in the document: the icon ops of `note` and `attachment` nodes,
//! and of note and attachment runs in text.

use crate::icons::{IconKind, IconOp};
use crate::layout::Geometry;
use crate::paint::{ContentCtx, Op};
use crate::schema::{Attachment, Note};

/// Emits the icon op of `note` at the top-left corner of the content box
/// of `geo`, on the page where it starts.
pub(crate) fn paint(note: &Note, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
    if let Some(at) = corner(geo, ctx) {
        paint_at(note, at, ops);
    }
}

/// [`paint`] for an attachment's icon.
pub(crate) fn paint_attachment(
    a: &Attachment,
    geo: &Geometry,
    ctx: &ContentCtx<'_>,
    ops: &mut Vec<Op>,
) {
    if let Some(at) = corner(geo, ctx) {
        paint_attachment_at(a, at, ops);
    }
}

/// The top-left corner of the content box of `geo`, in page coordinates,
/// on the page where it starts; `None` on another page.
fn corner(geo: &Geometry, ctx: &ContentCtx<'_>) -> Option<(f32, f32)> {
    let b = geo.content_box();
    let top = b.y + ctx.shift;
    if ctx.window.is_some_and(|w| !w.holds(top)) {
        return None;
    }
    Some((b.x + ctx.dx, top + ctx.dy))
}

/// [`paint_at`] for an attachment's icon.
pub(crate) fn paint_attachment_at(a: &Attachment, (x, y): (f32, f32), ops: &mut Vec<Op>) {
    ops.push(Op::Icon(Box::new(IconOp {
        x,
        y,
        kind: IconKind::Attachment(a.clone()),
    })));
}

/// Emits the icon op of `note` with its top-left corner at `(x, y)`, in
/// page coordinates: a note in a line of text hangs from the line's top.
pub(crate) fn paint_at(note: &Note, (x, y): (f32, f32), ops: &mut Vec<Op>) {
    ops.push(Op::Icon(Box::new(IconOp {
        x,
        y,
        kind: IconKind::Note(note.clone()),
    })));
}
