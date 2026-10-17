#!/usr/bin/env python3
"""Generates crates/core/src/text/vertical.rs, the Unicode
Vertical_Orientation property (UAX #50) that mixed orientation in vertical
SVG text follows, from a copy of the Unicode Character Database's
VerticalOrientation.txt (not in the repository: it comes with the
Unicode version, from https://www.unicode.org/Public/UCD/latest/ucd/).

  scripts/gen-vertical-orientation.py VerticalOrientation.txt "Unicode 17.0.0"

Lines are `start..end ; value` or `code ; value`, with `#` comments; code
points not listed are R (rotated).
"""
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parent.parent
out = root / "crates/core/src/text/vertical.rs"

if len(sys.argv) != 3:
    sys.exit(__doc__)
source, version = pathlib.Path(sys.argv[1]), sys.argv[2]

KINDS = {"U": "Upright", "Tu": "TransformedUpright", "Tr": "TransformedRotated"}
ranges = []
for line in source.read_text().splitlines():
    line = line.split("#", 1)[0].strip()
    if not line or line.startswith("@"):
        continue
    codes, value = (p.strip() for p in line.split(";"))
    if value == "R":
        continue
    if value not in KINDS:
        sys.exit(f"unknown value {value!r}")
    lo, _, hi = codes.partition("..")
    lo, hi = int(lo, 16), int(hi or lo, 16)
    if ranges and ranges[-1][1] + 1 == lo and ranges[-1][2] == value:
        ranges[-1][1] = hi
    else:
        ranges.append([lo, hi, value])
ranges.sort()

def hex_(v):
    """`v` as a Rust literal, grouped by four digits past the BMP."""
    return f"0x{v:04X}" if v <= 0xFFFF else f"0x{v >> 16:04X}_{v & 0xFFFF:04X}"


rows = "\n".join(f"    ({hex_(lo)}, {hex_(hi)}, {KINDS[v]})," for lo, hi, v in ranges)
out.write_text(f"""//! The Unicode `Vertical_Orientation` property (UAX #50), which says how
//! a character stands in vertical text with mixed orientation. Generated
//! by `scripts/gen-vertical-orientation.py` from `VerticalOrientation.txt`
//! ({version}); do not edit.

/// How a character stands in vertical text, when it is not `R` (turned
/// sideways, the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Orientation {{
    /// `U`: upright, as it is.
    Upright,
    /// `Tu`: upright, in its vertical form when the font has one.
    TransformedUpright,
    /// `Tr`: in its vertical form when the font has one, sideways
    /// otherwise.
    TransformedRotated,
}}

use Orientation::{{TransformedRotated, TransformedUpright, Upright}};

/// The characters that are not `R`, by range, in order.
const RANGES: &[(u32, u32, Orientation)] = &[
{rows}
];

/// The orientation of `c`, `None` for `R`.
pub(crate) fn orientation(c: char) -> Option<Orientation> {{
    let c = u32::from(c);
    let i = RANGES.partition_point(|r| r.1 < c);
    RANGES.get(i).filter(|r| r.0 <= c).map(|r| r.2)
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn reads_the_ranges() {{
        assert_eq!(orientation('a'), None);
        assert_eq!(orientation('§'), Some(Upright));
        assert_eq!(orientation('縦'), Some(Upright));
        assert_eq!(orientation('、'), Some(TransformedUpright));
        assert_eq!(orientation('「'), Some(TransformedRotated));
        assert_eq!(orientation('ー'), Some(TransformedRotated));
        assert_eq!(orientation('\\u{{10FFFF}}'), None);
        assert!(RANGES.windows(2).all(|w| w[0].1 < w[1].0));
    }}
}}
""")
print(f"{out.relative_to(root)}: {len(ranges)} ranges")
