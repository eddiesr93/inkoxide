//! Floats, and what text wraps around them.
//!
//! [`FloatContext`] places the floats of one container (CSS 2.2 §9.5.1,
//! for rectangles) and answers how much room they take beside a range of
//! heights. A paragraph's lines are as wide as its content box, less, over
//! the vertical ranges where a float sits beside it, the room that takes
//! on either side: [`LineBoxes`] holds those ranges; without any, every
//! line is as wide as the content box.

use std::sync::Arc;

use super::width_key;
use crate::schema::Clear;

/// A vertical range of a paragraph's content box with room taken on its
/// sides, in points from the top and from each side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Band {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

/// The box of one line in a paragraph's content box: its left edge and
/// its width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineBox {
    pub left: f32,
    pub width: f32,
}

/// The room taken on each side of a range of heights, from that side of
/// the content box.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Insets {
    pub left: f32,
    pub right: f32,
}

impl Insets {
    /// Whether no room is taken.
    pub fn is_empty(self) -> bool {
        self.left == 0.0 && self.right == 0.0
    }

    /// The room left between the insets in a box `width` wide.
    pub fn room(self, width: f32) -> f32 {
        width - self.left - self.right
    }

    /// The room both take, on each side.
    pub fn max(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            right: self.right.max(other.right),
        }
    }
}

/// The boxes a paragraph's lines go in: its width, and the bands where
/// less of it is free, sorted by `top`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBoxes {
    pub width: f32,
    pub bands: Arc<[Band]>,
}

impl LineBoxes {
    /// Lines as wide as `width`, all of them.
    pub fn plain(width: f32) -> Self {
        Self {
            width,
            bands: Arc::from([]),
        }
    }

    /// Whether any line can be narrower than the width.
    pub fn banded(&self) -> bool {
        !self.bands.is_empty()
    }

    /// The box of a line from `y` down to `y + h`, after the room every
    /// band it meets takes.
    pub fn at(&self, y: f32, h: f32) -> LineBox {
        let mut taken = Insets::default();
        for b in self.bands.iter().take_while(|b| b.top < y + h) {
            if b.bottom > y {
                taken = taken.max(Insets {
                    left: b.left,
                    right: b.right,
                });
            }
        }
        LineBox {
            left: taken.left,
            width: taken.room(self.width).max(0.0),
        }
    }

    /// The bottom of the lowest band: from there down, every line is as
    /// wide as the content box.
    pub fn end(&self) -> f32 {
        self.bands.iter().map(|b| b.bottom).fold(0.0, f32::max)
    }

    /// The first band edge below `y`, where a line that does not fit at `y`
    /// may fit; `None` once no band is left below.
    pub fn next_change(&self, y: f32) -> Option<f32> {
        self.bands
            .iter()
            .flat_map(|b| [b.top, b.bottom])
            .filter(|&e| e > y)
            .min_by(f32::total_cmp)
    }

    /// Whether these are the boxes of `other`: the same width, as cache
    /// keys compare widths, and the same bands.
    pub fn same(&self, other: &Self) -> bool {
        width_key(self.width) == width_key(other.width) && self.bands == other.bands
    }
}

/// The side a float goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// A float placed in its container: its side and its margin box, in the
/// container's content box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    pub side: Side,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Placed {
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Height a zero-height float or box counts as when looking for room, so
/// that it still meets the floats at its top.
const HAIR: f32 = 1e-3;

/// The floats of one container, in its content box `width` wide.
#[derive(Debug, Clone)]
pub(crate) struct FloatContext {
    width: f32,
    /// The floats that may still sit beside what comes next.
    floats: Vec<Placed>,
    /// The lowest bottom of the left and of the right floats dropped from
    /// `floats`, which only `clear` still looks at.
    retired: [Option<f32>; 2],
    /// The top of the last float placed: a later float goes no higher
    /// (CSS 2.2 §9.5.1, rule 5).
    last_top: f32,
}

impl FloatContext {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            floats: Vec::new(),
            retired: [None; 2],
            last_top: f32::NEG_INFINITY,
        }
    }

    /// The room the floats take beside `y..y + h`: on the left, up to the
    /// right edge of the left floats it meets; on the right, from the left
    /// edge of the right floats.
    pub fn band(&self, y: f32, h: f32) -> Insets {
        let h = h.max(HAIR);
        let mut taken = Insets::default();
        for f in &self.floats {
            if f.y < y + h && f.bottom() > y {
                match f.side {
                    Side::Left => taken.left = taken.left.max(f.x + f.w),
                    Side::Right => taken.right = taken.right.max(self.width - f.x),
                }
            }
        }
        taken
    }

    /// Drops the floats whose bottom is at or above `y`, which nothing
    /// from `y` down meets: the caller asks about nothing above `y` again.
    pub fn retire(&mut self, y: f32) {
        let retired = &mut self.retired;
        self.floats.retain(|f| {
            if f.bottom() > y {
                return true;
            }
            let side = &mut retired[usize::from(f.side == Side::Right)];
            *side = Some(side.map_or(f.bottom(), |b| b.max(f.bottom())));
            false
        });
    }

    /// The first float bottom below `y`: where more room may open. `None`
    /// once no float reaches below `y`.
    pub fn next_change(&self, y: f32) -> Option<f32> {
        self.floats
            .iter()
            .map(Placed::bottom)
            .filter(|&b| b > y)
            .min_by(f32::total_cmp)
    }

    /// The lowest bottom of the floats `clear` moves below, or `None`.
    pub fn clear_y(&self, clear: Clear) -> Option<f32> {
        let (left, right) = match clear {
            Clear::Left => (true, false),
            Clear::Right => (false, true),
            Clear::Both => (true, true),
            Clear::None => return None,
        };
        let cleared = |side: Side| match side {
            Side::Left => left,
            Side::Right => right,
        };
        self.floats
            .iter()
            .filter(|f| cleared(f.side))
            .map(Placed::bottom)
            .chain(
                [Side::Left, Side::Right]
                    .into_iter()
                    .filter(|&s| cleared(s))
                    .filter_map(|s| self.retired[usize::from(s == Side::Right)]),
            )
            .max_by(f32::total_cmp)
    }

    /// Where a top of at least `min_y` finds `w` of room over `h`, and the
    /// room the floats take there: moving down past float bottoms until it
    /// does, or until no float is left. Each step goes to a float bottom
    /// below the last, so it ends.
    pub fn fit(&self, w: f32, h: f32, min_y: f32) -> (f32, Insets) {
        let mut y = min_y;
        loop {
            let taken = self.band(y, h);
            if w <= taken.room(self.width) + 1e-3 || taken.is_empty() {
                return (y, taken);
            }
            match self.next_change(y) {
                Some(next) => y = next,
                None => return (y, taken),
            }
        }
    }

    /// Places a float of margin box `w` × `h` on `side`, its top no higher
    /// than `min_y` (where it occurs in the flow) nor than earlier floats,
    /// and returns it.
    pub fn place(&mut self, side: Side, w: f32, h: f32, min_y: f32) -> Placed {
        let (y, taken) = self.fit(w, h, min_y.max(self.last_top));
        let x = match side {
            Side::Left => taken.left,
            Side::Right => self.width - taken.right - w,
        };
        let placed = Placed { side, x, y, w, h };
        self.last_top = y;
        self.floats.push(placed);
        placed
    }

    /// The bands the floats make in a box whose content box starts at
    /// (`x`, `y`) in the container and is `w` wide, relative to it: only
    /// the floats reaching below `y`, sorted by top.
    pub fn bands(&self, x: f32, y: f32, w: f32) -> Vec<Band> {
        let mut bands: Vec<Band> = self
            .floats
            .iter()
            .filter(|f| f.bottom() > y && f.h > 0.0)
            .filter_map(|f| {
                let (left, right) = match f.side {
                    Side::Left => ((f.x + f.w - x).max(0.0), 0.0),
                    Side::Right => (0.0, (x + w - f.x).max(0.0)),
                };
                (left > 0.0 || right > 0.0).then_some(Band {
                    top: f.y - y,
                    bottom: f.bottom() - y,
                    left,
                    right,
                })
            })
            .collect();
        bands.sort_by(|a, b| a.top.total_cmp(&b.top));
        bands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_stack_side_by_side_then_wrap_below() {
        let mut c = FloatContext::new(100.0);
        let a = c.place(Side::Left, 40.0, 20.0, 0.0);
        let b = c.place(Side::Left, 40.0, 30.0, 0.0);
        assert_eq!((a.x, a.y, b.x, b.y), (0.0, 0.0, 40.0, 0.0));
        // No room for a third beside them: below the shorter one, `b`
        // still takes the room it needs; below both, it fits.
        let d = c.place(Side::Left, 40.0, 10.0, 0.0);
        assert_eq!((d.x, d.y), (0.0, 30.0));
        // A right float goes no higher than `d`, and takes the room there.
        let r = c.place(Side::Right, 15.0, 10.0, 0.0);
        assert_eq!((r.x, r.y), (85.0, 30.0));
        let insets = |left, right| Insets { left, right };
        assert_eq!(c.band(0.0, 5.0), insets(80.0, 0.0));
        assert_eq!(c.band(30.0, 1.0), insets(40.0, 15.0));
        assert_eq!(c.next_change(0.0), Some(20.0));
        assert_eq!(c.next_change(40.0), None);
    }

    #[test]
    fn a_later_float_goes_no_higher_than_an_earlier_one() {
        let mut c = FloatContext::new(100.0);
        c.place(Side::Left, 30.0, 50.0, 10.0);
        let r = c.place(Side::Right, 30.0, 10.0, 0.0);
        assert_eq!(r.y, 10.0);
    }

    #[test]
    fn clear_goes_below_the_floats_of_its_sides() {
        let mut c = FloatContext::new(100.0);
        assert_eq!(c.clear_y(Clear::Both), None);
        c.place(Side::Left, 30.0, 50.0, 0.0);
        c.place(Side::Right, 30.0, 20.0, 0.0);
        assert_eq!(c.clear_y(Clear::Left), Some(50.0));
        assert_eq!(c.clear_y(Clear::Right), Some(20.0));
        assert_eq!(c.clear_y(Clear::Both), Some(50.0));
        assert_eq!(c.clear_y(Clear::None), None);
    }

    #[test]
    fn retired_floats_still_count_for_clear_only() {
        let mut c = FloatContext::new(100.0);
        c.place(Side::Left, 30.0, 50.0, 0.0);
        c.place(Side::Right, 30.0, 20.0, 0.0);
        c.retire(20.0);
        // The right float is done with: no longer active, still cleared.
        assert_eq!(c.floats.len(), 1);
        assert_eq!(c.clear_y(Clear::Right), Some(20.0));
        assert_eq!(
            c.band(20.0, 10.0),
            Insets {
                left: 30.0,
                right: 0.0
            }
        );
        c.retire(60.0);
        assert!(c.floats.is_empty());
        assert_eq!(c.next_change(0.0), None);
        assert_eq!(c.clear_y(Clear::Both), Some(50.0));
        assert_eq!(c.clear_y(Clear::Left), Some(50.0));
        // A later float still goes no higher than the earlier ones.
        assert_eq!(c.place(Side::Left, 10.0, 10.0, 0.0).y, 0.0);
    }

    #[test]
    fn zero_size_and_too_wide_floats_are_placed() {
        let mut c = FloatContext::new(100.0);
        let z = c.place(Side::Left, 0.0, 0.0, 5.0);
        assert_eq!((z.x, z.y), (0.0, 5.0));
        let a = c.place(Side::Left, 60.0, 10.0, 5.0);
        // Wider than the room beside `a`, and than the container: below
        // every float, at its side.
        let w = c.place(Side::Right, 150.0, 10.0, 5.0);
        assert_eq!((w.x, w.y), (-50.0, a.bottom()));
        // Bands of a box beside them, relative to its content box.
        let bands = c.bands(10.0, 5.0, 80.0);
        assert_eq!(
            bands[0],
            Band {
                top: 0.0,
                bottom: 10.0,
                left: 50.0,
                right: 0.0
            }
        );
    }

    fn boxes(bands: &[Band]) -> LineBoxes {
        LineBoxes {
            width: 100.0,
            bands: bands.into(),
        }
    }

    #[test]
    fn a_line_takes_the_room_of_every_band_it_meets() {
        let b = boxes(&[
            Band {
                top: 0.0,
                bottom: 20.0,
                left: 30.0,
                right: 0.0,
            },
            Band {
                top: 15.0,
                bottom: 40.0,
                left: 10.0,
                right: 25.0,
            },
        ]);
        let line = |left, width| LineBox { left, width };
        assert_eq!(b.at(0.0, 10.0), line(30.0, 70.0));
        assert_eq!(b.at(12.0, 10.0), line(30.0, 45.0));
        assert_eq!(b.at(20.0, 10.0), line(10.0, 65.0));
        assert_eq!(b.at(40.0, 10.0), line(0.0, 100.0));
        assert_eq!(b.end(), 40.0);
        assert_eq!(b.next_change(0.0), Some(15.0));
        assert_eq!(b.next_change(15.0), Some(20.0));
        assert_eq!(b.next_change(40.0), None);
        assert!(!LineBoxes::plain(10.0).banded());
        assert_eq!(LineBoxes::plain(10.0).at(5.0, 5.0), line(0.0, 10.0));
        assert_eq!(LineBoxes::plain(10.0).end(), 0.0);
    }
}
