//! Text notes: the icon ops of `note` nodes and of note runs in text.

use crate::icons::{IconKind, IconOp};
use crate::layout::Geometry;
use crate::paint::{ContentCtx, Op};
use crate::schema::Note;

/// Emits the icon op of `note` at the top-left corner of the content box
/// of `geo`, on the page where it starts.
pub(crate) fn paint(note: &Note, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
    let b = geo.content_box();
    let top = b.y + ctx.shift;
    if ctx.window.is_some_and(|w| !w.holds(top)) {
        return;
    }
    paint_at(note, (b.x + ctx.dx, top + ctx.dy), ops);
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
