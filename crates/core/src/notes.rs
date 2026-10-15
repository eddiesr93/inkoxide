//! Text notes: the icon ops of `note` nodes.

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
    ops.push(Op::Icon(Box::new(IconOp {
        x: b.x + ctx.dx,
        y: top + ctx.dy,
        kind: IconKind::Note(note.clone()),
    })));
}
