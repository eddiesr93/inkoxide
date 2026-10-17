//! Icons in the document: the icon ops of `note` and `attachment` nodes,
//! and of note and attachment runs in text.

use crate::icons::{Icon, IconOp};
use crate::layout::Geometry;
use crate::paint::{ContentCtx, Op};

/// Emits the op of `icon` at the top-left corner of the content box of
/// `geo`, on the page where it starts.
pub(crate) fn paint(icon: Icon<'_>, geo: &Geometry, ctx: &ContentCtx<'_>, ops: &mut Vec<Op>) {
    let b = geo.content_box();
    let top = b.y + ctx.shift;
    if ctx.window.is_some_and(|w| !w.holds(top)) {
        return;
    }
    paint_at(icon, (b.x + ctx.dx, top + ctx.dy), ops);
}

/// Emits the op of `icon` with its top-left corner at `(x, y)`, in page
/// coordinates: an icon in a line of text hangs from the line's top.
pub(crate) fn paint_at(icon: Icon<'_>, (x, y): (f32, f32), ops: &mut Vec<Op>) {
    ops.push(Op::Icon(Box::new(IconOp {
        x,
        y,
        kind: icon.to_kind(),
        owner: None,
    })));
}
