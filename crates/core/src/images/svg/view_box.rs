//! The root `viewBox`: how an SVG fills the box it is drawn in.

use svgtypes::{Align, AspectRatio, NumberListParser};
use usvg::roxmltree::Node;

use crate::geom::Rect;

/// The root `viewBox` of an SVG and how it fits a viewport of another
/// shape (`preserveAspectRatio`). `usvg` folds both into the tree's root
/// transform for the SVG's own size, so they are read here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewBox {
    /// `x`, `y`, width and height (both > 0), in user units.
    pub rect: [f32; 4],
    /// Where the `viewBox` sits in spare room, as a fraction of it per
    /// axis (`xMin` 0, `xMid` 0.5, `xMax` 1); `None` stretches it
    /// (`none`).
    pub align: Option<(f32, f32)>,
    /// `slice` (cover the viewport) rather than `meet` (fit inside it).
    pub slice: bool,
}

impl ViewBox {
    /// Reads the root element's `viewBox` and `preserveAspectRatio`, as
    /// `usvg` does. `None` without a usable `viewBox`: one of zero or
    /// negative size is none.
    pub(crate) fn of(svg: Node<'_, '_>) -> Option<Self> {
        let rect = Self::rect_of(svg)?;
        let [.., w, h] = rect;
        if !(w > 0.0 && h > 0.0) {
            return None;
        }
        // An invalid value is the default, `xMidYMid meet`.
        let ratio = svg
            .attribute("preserveAspectRatio")
            .and_then(|v| v.parse::<AspectRatio>().ok())
            .unwrap_or_default();
        let align = match ratio.align {
            Align::None => None,
            Align::XMinYMin => Some((0.0, 0.0)),
            Align::XMidYMin => Some((0.5, 0.0)),
            Align::XMaxYMin => Some((1.0, 0.0)),
            Align::XMinYMid => Some((0.0, 0.5)),
            Align::XMidYMid => Some((0.5, 0.5)),
            Align::XMaxYMid => Some((1.0, 0.5)),
            Align::XMinYMax => Some((0.0, 1.0)),
            Align::XMidYMax => Some((0.5, 1.0)),
            Align::XMaxYMax => Some((1.0, 1.0)),
        };
        Some(Self::new(rect, align, ratio.slice))
    }

    /// The four numbers of the root element's `viewBox`, as `usvg` reads
    /// them (no separator is needed before a sign), whatever their sign;
    /// `None` when they are not four finite numbers.
    pub(super) fn rect_of(svg: Node<'_, '_>) -> Option<[f32; 4]> {
        let mut numbers = NumberListParser::from(svg.attribute("viewBox")?);
        let mut next = || Some(numbers.next()?.ok()? as f32);
        let rect = [next()?, next()?, next()?, next()?];
        rect.iter().all(|n| n.is_finite()).then_some(rect)
    }

    pub(super) fn new(rect: [f32; 4], align: Option<(f32, f32)>, slice: bool) -> Self {
        Self { rect, align, slice }
    }

    /// The part of the viewport `(x, y, w, h)` that the `viewBox` covers.
    pub(crate) fn fit(&self, (x, y, w, h): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let [_, _, vw, vh] = self.rect;
        let Some((ax, ay)) = self.align else {
            return (x, y, w, h);
        };
        let (sx, sy) = (w / vw, h / vh);
        let s = if self.slice { sx.max(sy) } else { sx.min(sy) };
        let (fw, fh) = (vw * s, vh * s);
        (x + ax * (w - fw), y + ay * (h - fh), fw, fh)
    }

    /// Where to draw an SVG of `size` so that its `viewBox` fits `bx`, the
    /// SVG's viewport there (as CSS draws an SVG image with `object-fit:
    /// fill`), and whether to clip to `bx`.
    pub fn place(&self, size: (f32, f32), bx: Rect) -> (Rect, bool) {
        let (w, h) = size;
        let (fx, fy, fw, fh) = self.fit((0.0, 0.0, w, h));
        let (tx, ty, tw, th) = self.fit((bx.x, bx.y, bx.w, bx.h));
        let (sx, sy) = (tw / fw, th / fh);
        let r = Rect::new(tx - fx * sx, ty - fy * sy, w * sx, h * sy);
        let rect = if [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) {
            r
        } else {
            bx
        };
        (rect, rect.overflows(bx))
    }
}
