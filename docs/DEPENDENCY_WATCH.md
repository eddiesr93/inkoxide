# Dependency watch

Two pinned dependencies are flagged by RustSec as unmaintained (not
vulnerable). ADR-020 keeps them for v1 and plans a swap. This page records
what to watch and when to act. `scripts/dep-watch.sh` prints the current
versions:

```
$ scripts/dep-watch.sh
crate        pinned     latest     updated
rustybuzz    0.20.1     0.20.1     2024-11-12
ttf-parser   0.25.1     0.25.1     2024-11-29
-- successors
harfrust     -          0.14.0     2026-10-04
read-fonts   -          0.45.0     2026-10-03
skrifa       -          0.48.0     2026-10-03
```

(Output as of 2026-10-08.)

## The two migrations

| | Shaping | Font parsing |
|---|---|---|
| Current pin | `rustybuzz =0.20.1` | `ttf-parser =0.25.1` |
| Advisory | RUSTSEC-2026-0206 | RUSTSEC-2026-0192 |
| Successor | `harfrust` (HarfBuzz project; a maintained rustybuzz fork) | `read-fonts` (Google fontations) |
| Latest successor | 0.14.0 (2026-10-04) | 0.45.0 (2026-10-03) |
| What we use today | `Face::from_slice`, `UnicodeBuffer`, `ShapePlan`, `shape_with_plan` | `Face::parse`, the `name` table (PostScript name), the raw `OS/2` table, `glyph_hor_advance`, `number_of_glyphs`, units per em, ascender/descender, global bounding box, underline/strikeout metrics, the variable-font and CFF2 checks |
| Files touched | `crates/core/src/text/prepare.rs` (shape-plan cache and the `shape` call), `crates/core/src/fonts/mod.rs` (`LoadedFont` holds a `rustybuzz::Face`) | `crates/core/src/fonts/mod.rs` (registration and metrics), `crates/core/src/pdf/cff.rs` (glyph count) |
| Also update | the `text/mod.rs` module docs; `deny.toml` (drop the ignore) | `deny.toml` (drop the ignore) |

harfrust is built on read-fonts (it shapes read-fonts font data), so
the two migrations are most naturally done together, in one commit, after which `ttf-parser` and
`rustybuzz` leave the dependency tree. `skrifa` (glyph outlines and
hinting) is not needed: we read metrics and tables only, and `subsetter`
does the subsetting.

## When to migrate

Migrate when all of these hold:

1. **A settled release.** The successor has a release that has been out
   for at least 8 weeks with no breaking release after it, or a 1.0. On
   2026-10-08 neither holds: harfrust 0.14.0 and read-fonts 0.45.0 are
   days old, and both make breaking (0.x minor) releases every few weeks.
2. **Same output.** Every fixture renders with the same line breaks: the
   visual suite (`scripts/visual-diff.sh`) passes against the current
   expected PNGs. Byte-identical PDFs are not required (glyph advances
   may round differently), but any changed expected PNG needs a reason
   in the commit.
3. **No slowdown.** The headline benchmark (`scripts/bench-headline.sh`)
   and the table dump (docs/BENCHMARKS.md) are within 5% of the numbers
   recorded there.
4. **Allowlist.** The new crates are added to the allowlist through an
   ADR that supersedes ADR-020, pinned `=x.y.z`, and `cargo deny check`
   is green without either ignore.

Until then, the ignores stay in `deny.toml`, scoped to exactly the two
advisory IDs above, each with a reason. Any other advisory still fails
`cargo deny` (and `scripts/ci.sh`).

Act sooner if either pinned crate gets a **vulnerability** advisory (not
just "unmaintained"), or if a font we must support fails to parse: then
the migration is the fix, and criterion 1 is waived in the ADR.

## How to check

- Run `scripts/dep-watch.sh` (needs network access to crates.io).
- `cargo deny check advisories` shows whether new advisories appeared.
- The changelogs: `harfrust` at github.com/harfbuzz/harfrust,
  `read-fonts` at github.com/googlefonts/fontations.
