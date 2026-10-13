//! What text wraps around: the boxes a paragraph's lines are laid in.
//!
//! A paragraph's lines are as wide as its content box, less, over the
//! vertical ranges where something sits beside it (a float), the room
//! that takes on either side. [`LineBoxes`] holds those ranges; without
//! any, every line is as wide as the content box.

use std::sync::Arc;

use super::width_key;

/// A vertical range of a paragraph's content box with room taken on its
/// sides, in points from the top and from each side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Band {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
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

    /// The box of a line from `y` down to `y + h`: its left edge and its
    /// width, after the room every band it meets takes.
    pub fn at(&self, y: f32, h: f32) -> (f32, f32) {
        let (mut left, mut right) = (0.0f32, 0.0f32);
        for b in self.bands.iter().take_while(|b| b.top < y + h) {
            if b.bottom > y {
                left = left.max(b.left);
                right = right.max(b.right);
            }
        }
        (left, (self.width - left - right).max(0.0))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(b.at(0.0, 10.0), (30.0, 70.0));
        assert_eq!(b.at(12.0, 10.0), (30.0, 45.0));
        assert_eq!(b.at(20.0, 10.0), (10.0, 65.0));
        assert_eq!(b.at(40.0, 10.0), (0.0, 100.0));
        assert_eq!(b.next_change(0.0), Some(15.0));
        assert_eq!(b.next_change(15.0), Some(20.0));
        assert_eq!(b.next_change(40.0), None);
        assert!(!LineBoxes::plain(10.0).banded());
        assert_eq!(LineBoxes::plain(10.0).at(5.0, 5.0), (0.0, 10.0));
    }
}
