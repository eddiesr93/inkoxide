//! The root `viewBox`: how an SVG fills the box it is drawn in.

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
    /// Reads the root element's `viewBox` and `preserveAspectRatio`.
    /// `None` without a usable `viewBox`.
    pub(crate) fn of(svg: Node<'_, '_>) -> Option<Self> {
        let v: Vec<f32> = svg
            .attribute("viewBox")?
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        let &[x, y, w, h] = v.as_slice() else {
            return None;
        };
        if !(v.iter().all(|n| n.is_finite()) && w > 0.0 && h > 0.0) {
            return None;
        }
        let mut words = svg
            .attribute("preserveAspectRatio")
            .unwrap_or("")
            .split_ascii_whitespace()
            .skip_while(|&w| w == "defer");
        let align = match words.next() {
            Some("none") => None,
            Some(a) if a.len() == 8 && a.starts_with('x') && a[4..].starts_with('Y') => {
                let frac = |s: &str| match s {
                    "Min" => Some(0.0),
                    "Mid" => Some(0.5),
                    "Max" => Some(1.0),
                    _ => None,
                };
                // An invalid value is the default, `xMidYMid meet`.
                let Some(pair) = frac(&a[1..4]).zip(frac(&a[5..8])) else {
                    return Some(Self::new([x, y, w, h], Some((0.5, 0.5)), false));
                };
                Some(pair)
            }
            _ => Some((0.5, 0.5)),
        };
        Some(Self::new(
            [x, y, w, h],
            align,
            words.next() == Some("slice"),
        ))
    }

    pub(super) fn new(rect: [f32; 4], align: Option<(f32, f32)>, slice: bool) -> Self {
        Self { rect, align, slice }
    }

    /// The part of `size` (the viewport, from the origin) the `viewBox`
    /// covers.
    pub(super) fn fit(&self, (x, y, w, h): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
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
