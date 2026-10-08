# Architecture decision log

Format: context → decision → consequences. Newest last. Anything not
obvious from the build brief goes here.

## ADR-001 — No hosted CI; checks run locally (M0)

**Context.** The brief specifies GitHub Actions for CI and releases. The
project owner asked that no Actions minutes be used for now.

**Decision.** No `.github/workflows`. `scripts/ci.sh` runs every check
(fmt, clippy `-D warnings`, tests, schema conformance, `qpdf --check`,
cargo-deny, visual diff) and must pass before pushing. The release
matrix (M5) will be written as a script/docs, not a workflow, until hosted
CI is approved.

**Consequences.** Linux-only verification for now (no macOS/Windows runs).
Re-introducing workflows later is mechanical: each workflow step maps to a
step of `scripts/ci.sh`.

## ADR-002 — JSON input in v1 (M0)

**Context.** JSON vs a binary encoding. **Decision.** JSON (serde_json) in
v1; MessagePack later behind `format`, sharing the same serde structs.
**Consequences.** Debuggable fixtures and error paths; parse cost is small
next to layout (measure in M6).

## ADR-003 — Nodes deserialise through one flat raw struct (M0)

**Context.** serde's internally tagged enums (`#[serde(tag = "type")]`)
buffer each node into an intermediate `Content` value. That copies nested
subtrees once per depth level and, worse, loses the error path below the
first tagged enum, so `serde_path_to_error` could only point at the node.

**Decision.** `RawNode` holds every field any node type can carry, with
`deny_unknown_fields`, and is converted (`try_from`) into the typed `Node`.
Fields that exist but are not valid for the node's type produce
`UNKNOWN_FIELD` at the field's own path.

**Consequences.** Precise paths (`pages[0].children[3].style.colour`), no
buffering. The schema-parity test checks that the union of node properties
in the JSON Schema equals the raw struct's fields.

## ADR-004 — `serde_path_to_error` dependency (M0)

**Context.** Errors need a JSON path (§4.8); serde alone gives line/column.
**Decision.** Add `serde_path_to_error` (dtolnay, ~500 lines, no
transitive deps beyond serde). **Consequences.** Works with any serde
deserializer, so MessagePack input gets paths too.

## ADR-005 — Structured codes from serde errors (M0)

**Context.** Serde errors are strings. **Decision.** Our own deserialisers
prefix messages with `[CODE]` or `[CODE@field]`; serde's built-in messages
(`unknown field`, `missing field`, `invalid type`, …) are classified by
prefix. All of it lives in `validate::classify`. **Consequences.** One
small, tested mapping; serde message wording changes would show up in the
validation tests.

## ADR-006 — Schema shape choices (M0)

Choices the brief left open, made when writing `document.v1.json`:

- **Edges are objects**, `{top, right, bottom, left}` with each side
  optional, for `padding`, `margin`, `borderWidth`, `borderColor` and
  page `margin`. One shape everywhere, no shorthand precedence rules.
- **Positioned offsets** live in `inset: {top, right, bottom, left}`
  rather than four top-level style keys, for the same reason.
- **`gap`** sets both gaps; `rowGap`/`columnGap` override it (CSS rule).
- **`flexDirection` defaults to `column`** (document flow is vertical;
  react-pdf does the same). Everything else follows CSS defaults.
- **Lengths** accept `"auto"` besides points and percentages wherever CSS
  does (`width`, `height`, `flexBasis`, `margin`, `inset`, min/max).
- **Grid placement** is `{start, end, span}` with 1-based lines, not CSS
  strings.
- **Header/footer placement**: laid out at the full page width; header at
  y = 0, footer bottom-aligned to the page. Insets come from their own
  padding; margins must leave room.
- **Placeholder runs** carry `placeholder` *instead of* `text` (exactly one
  of the two).
- **Text-only style properties on other nodes are errors**
  (`STYLE_NOT_APPLICABLE`), consistent with "unknown properties are
  errors": silent ignoring hides client bugs.
- **`break` is only valid in the flow**, not inside headers, footers or
  table cells, where it would have no meaning.
- **Image intrinsic size** is 96 dpi (1 px = 0.75 pt), matching CSS, and
  `fit` defaults to `fill` like CSS `object-fit`.
- **Table rows** carry `minHeight` and `backgroundColor` directly; cell
  styling goes on cells (`style`, including `verticalAlign`).

## ADR-007 — No creation date unless asked (M0)

**Context.** Determinism (§1.2) vs a useful Info dictionary.
**Decision.** `/CreationDate` is written only when `options.creationDate`
or `meta.creationDate` is set; there is no implicit "now", and the core
never reads the clock. No `/ID` is written. **Consequences.** Output is
byte-identical by default; callers that want a date pass one.

## ADR-008 — Top-left display-list coordinates (M0)

**Decision.** Layout and the display list use a top-left origin with y down.
Each content stream starts with one `1 0 0 -1 0 H cm` flip; text uses a
flipped text matrix and images a flipped image matrix. All numbers are
rounded to 1/1000 pt before writing (≤ 3 decimals; pdf-writer then prints
the shortest round-trip form).

## ADR-009 — Taffy through its high-level tree, rounding off (M1)

**Context.** Taffy can run on its own `TaffyTree` or on a custom tree that
implements its traits. **Decision.** Use `TaffyTree<NodeIdx>` built from our
BFS arena (back to front, so children exist before parents), with
`disable_rounding()` — PDF coordinates are fractional and pixel snapping
would only add error. Leaves carry their arena index as context; the
measure closure dispatches through our `LeafMeasure` trait. Absolute
geometry is copied back into the arena and the Taffy tree is dropped.
**Consequences.** ~100 lines of glue behind `LayoutEngine`. If profiling
(M6) shows tree construction matters, implement Taffy's traits directly on
the arena without changing callers.

## ADR-010 — Opacity via ExtGState, not transparency groups (M1)

**Context.** CSS `opacity` composites a subtree as a group. PDF's `ca/CA`
apply per painted object, so overlapping children of a translucent box
show through each other; true group opacity needs a Form XObject with a
transparency `/Group`. **Decision.** v1 uses `ExtGState` alpha. The painter
tracks cumulative opacity (PDF `gs` sets an absolute value, it does not
multiply) and one shared `ExtGState` exists per distinct value (quantised
to 1/1000). Colour alpha folds into the same mechanism. **Consequences.**
Correct for the common case (a translucent box with non-overlapping
content). Group opacity becomes a form XObject later if needed (M7).

## ADR-011 — Border model (M1)

**Decision.** Borders are painted inside the border box. With one colour
and solid style, the border is a single even-odd fill of the ring between
the outer (border-box) and inner (padding-box) rounded rectangles; inner
radii are the outer radii minus the adjacent widths. With several colours,
each side is a mitred trapezoid clipped by that ring, so corners and radii
come out right without per-corner geometry. Dashed/dotted borders stroke
the middle of each side (dash `3w/3w`, dots as zero-length dashes with
round caps); a uniform dashed border with radius strokes one rounded path.
Default border colour is black (CSS `currentColor` has no meaning here).
**Consequences.** Dashed borders with mixed widths and radii are
approximate at the corners; acceptable for v1.

## ADR-012 — M1 overflow: clip with a warning (M1)

**Decision.** Until pagination (M3), each page template renders on one page
and content taller than the content box is clipped to it, with
`tracing::warn!`. Replaced by real pagination in M3.

## ADR-013 — Own paragraph layout on rustybuzz instead of cosmic-text (M2)

**Context.** The brief picks cosmic-text for shaping, wrapping and
fallback. cosmic-text 0.19 (current) no longer uses rustybuzz: it shapes
with `harfrust` and hard-depends on `skrifa`, which the brief bans, plus
`fontdb`, `swash` and `sys-locale`. It also resolves fonts by family,
weight and style (not by our font id), and runs its own fallback lists
rather than the document's `fallbackFonts` order.

**Decision.** `text/` implements paragraph layout directly on the
allowlisted `rustybuzz` (shaping) and `ttf-parser` (metrics), with
`unicode-linebreak` (UAX #14 break opportunities; zero dependencies and
already a transitive dependency of cosmic-text) and `hypher`. About 900
lines in total:

- `prepare`: whitespace processing → soft-hyphen insertion → per-character
  font itemisation (run font, then `fallbackFonts` in order, marks and
  spaces stick to the previous font) → shaping per segment. Width
  independent, cached per node.
- `lines`: greedy first-fit breaking over prefix sums of advances,
  trailing spaces hang, unbreakable words break between clusters in
  `normal` and overflow in `pre`/`nowrap`, `maxLines`/ellipsis, line boxes
  with CSS half-leading.
- `paint`: alignment, justification (extra space on U+0020/U+00A0
  glyphs), decorations from the font's `post`/`OS/2` metrics,
  placeholders.

All of it sits behind `TextShaper`. **Consequences.** Exact font
selection and fallback order, deterministic output, visible hyphens under
our control (ADR-015). No bidi/complex-script line layout beyond what
rustybuzz shapes (a v1 non-goal). rustybuzz has not had a release since
Nov 2024; if that matters, `harfrust` can replace it inside
`text/prepare.rs` only.

## ADR-014 — TrueType outlines only; CFF and variable fonts rejected (M2)

**Decision.** `FontStore::register` rejects fonts without a `glyf` table
(CFF-based OpenType) and variable fonts, with a clear `FONT_ERROR`.
Embedding is `Type0`/`Identity-H` with a `CIDFontType2` descendant and the
full font program as `FontFile2` (Flate). **Consequences.** CFF support
(`FontFile3`) is M7. Most web fonts ship TrueType builds. *Amended by
ADR-037:* static CFF fonts are accepted; variable fonts and CFF2 are
still rejected.

## ADR-015 — Hyphenation: soft hyphens + our own hyphen glyph; no Romanian patterns yet (M2)

**Context.** The brief assumed `hypher` includes Romanian. hypher 0.1.8
supports ~50 languages but **not Romanian**. The standard TeX patterns
(`hyph-ro.tex`, hyph-utf8) state no licence ("licence: [None]"), which is
presumably why hypher omits them.

**Decision.** When `hyphenate: true`, words longer than five letters get
U+00AD at hypher's break points before shaping. SHY glyphs are invisible
and zero-width. When a line breaks at a SHY, the line layout appends
U+2010 (or `-`) from the font of the preceding glyph and counts its width
when fitting. This settles the "visible hyphen at SHY" spike from the
brief: we never depend on the shaper to draw it. For languages hypher
lacks (including `ro`), `hyphenate` is a no-op.

**Open question (needs the project owner).** For Romanian, either
(a) vendor `hyph-ro.tex` after clarifying its licence, (b) let clients
register TeX patterns at runtime (new public API), or (c) accept no
Romanian hyphenation. The `Hyphenator` boundary makes (a) or (b) a small,
local change.

**Resolved (post-M7).** The owner chose (b); see ADR-044.

## ADR-016 — Newlines in `whiteSpace: normal` are forced breaks (M2)

**Context.** CSS `normal` collapses newlines into spaces; react-pdf breaks
lines at `\n`. **Decision.** Follow react-pdf: in `normal` and `nowrap`,
`\n` forces a line break and other whitespace collapses (CSS `pre-line`).
In `pre`, everything is preserved (tabs become four spaces) and only `\n`
breaks. A collapsed space stays in the run where it first occurs, so
decorations do not start with a space.

## ADR-017 — Font program compression cached per registered font (M2)

**Decision.** `FontData` compresses the font program once (Flate level 6)
and keeps it behind a `OnceLock`, so every later render that embeds the
font copies bytes instead of re-compressing ~400 KB. **Consequences.**
First use of a font costs a few ms; later renders do not. With full
embedding (v1) every PDF carries the whole font, so subsetting (M7) is the
next big win for file size.

## ADR-018 — Flow root has the page content height as its minimum (M2)

**Context.** The flow is laid out in an unbounded column for pagination,
so a `flexGrow` spacer had nothing to grow into (react-pdf pins footers
this way). **Decision.** The root's `min-height` is the page content
height. Short documents fill the page; longer flows are unaffected.

## ADR-019 — Text measurement is cached by exact width (M2)

**Context.** The brief suggests 0.5 pt width buckets. Rounding a width
down to its bucket can drop it below the paragraph's min-content width and
break a word that fits. **Decision.** Shaping is cached per node (width
independent); line layouts are cached per exact width. Taffy asks for few
distinct widths per node, so the hit rate is the same. Cross-node
memoisation by content hash (identical table cells) is left to M6,
driven by profiles.

## ADR-020 — rustybuzz and ttf-parser are unmaintained: keep for v1, plan the swap (M2)

**Context.** RustSec now lists both crates as unmaintained:
RUSTSEC-2026-0206 (rustybuzz, successor `harfrust` from the HarfBuzz
project) and RUSTSEC-2026-0192 (ttf-parser, successor the fontations
crates). These are the brief's own picks, and `cargo deny` fails on
them.

**Options.** (1) Swap now to `harfrust` + `read-fonts`. harfrust shapes
on read-fonts' new `model::Font` (owned, `Arc`-shared, which is nicer than
rustybuzz's borrowed face), and read-fonts covers our metric needs without
`skrifa` (still banned). But harfrust 0.14 was published four days before
this decision and the read-fonts `model` API is weeks old. (2) Keep the
frozen, proven crates for v1 and swap later.

**Decision.** (2) for now. The two advisories are ignored in `deny.toml`
with reasons pointing here. "Unmaintained" is not a vulnerability, and
both crates are pure Rust and `forbid(unsafe_code)`. The blast radius of
a swap is `fonts/mod.rs` (parse + metrics + cmap/advance lookups) and
`text/prepare.rs` (one `shape` call), so it is a one-module change, as the
brief intends.

**Follow-up.** Migrate once harfrust/read-fonts have a release that has
been out for a few weeks (target: M6, with before/after benchmarks and the
visual suite as the safety net). The project owner may prefer to do it
sooner.

**Update (post-M7).** Not done in M6: harfrust and read-fonts were still
making breaking releases every few weeks. The criteria for doing it and
the files it touches are in [DEPENDENCY_WATCH.md](DEPENDENCY_WATCH.md);
`scripts/dep-watch.sh` prints the current versions.

## ADR-021 — Pagination by shift accumulation over one layout (M3)

**Decision.** Each page template's flow is laid out once (unbounded
column, §6.4). `paginate` walks it in document order with a running
`shift`; each node records `shift_top`/`shift_bottom`, and split
paragraphs record one shift per line. Paged position = layout position +
shift; page `k` shows `[k·H, (k+1)·H)`. Rules:

- **Head rule.** Before placing a node, its *head* (height up to the end
  of its first unbreakable piece: first line, first child's head, or the
  whole node if atomic) must fit, together with `minPresenceAhead`;
  otherwise the whole node moves to the next page. This prevents empty
  slivers (a box's top padding/border alone at a page bottom) and is how
  `minPresenceAhead` works.
- **Atomic nodes** move whole: `inside: avoid`, row / row-reverse /
  column-reverse / wrapping flex boxes, grid boxes (the brief's v1 rule
  for rows extended to every layout whose children are not stacked
  vertically), images, spacers, empty boxes, tables until M4, positioned,
  rotated and fixed nodes. Taller than a page, they are sliced and a
  warning is logged.
- **Forced breaks** (`before/after: page`, `pageBreak`) never create an
  empty page: a break at the top of a page is a no-op.
- **Painting** clips each page to its content band (full page width) and
  paints only nodes whose page range includes the page and only the lines
  in the page's window. Split boxes are therefore sliced at the page edge
  (CSS `box-decoration-break: slice`): background on every fragment, top
  border on the first, bottom border on the last.

**Consequences.** Pagination is O(nodes + lines), with no re-layout.
Splitting rows of columns is not supported in v1. No widow/orphan line
control beyond the head rule.

## ADR-022 — Headers, footers and fixed nodes (M3)

**Decision.** Header and footer are laid out once per page template at the
full page width (their own tree, no minimum height). The header is
painted at y = 0, the footer bottom-aligned, on every page of the
template, after the flow, so they sit above it. Nodes with `break.fixed`
keep their layout space in the flow and are painted on every page of
their template at the same page-relative position as on their first page,
above the flow. Page numbers are global across templates: `pageNumber`
is the physical page index (1-based) and `totalPages` the document's
page count, both substituted at paint time.

## ADR-023 — Tables: own layout, cells as Taffy subtrees, collapsed borders (M4)

**Decision.** In the arena a table is table → rows → cells → content. For
Taffy the table is a leaf whose measure runs `layout/table.rs`: fixed /
`fr` / `auto` columns (auto = max-content of single-span cells, shrunk
towards min-content when short), row height = tallest single-row cell
(at least `minHeight`), row-span deficits added to the span's last row.
Each cell is laid out as its own small Taffy subtree (flex column, height
fixed to its rows, `verticalAlign` → `justify-content`). After the flow
layout, tables are laid out at their final width in arena order (outer
before nested) and their rows, cells and contents get absolute geometry.
Layouts are cached per (table, width) and per (cell, width).

**Borders** use the collapsed model only: each grid edge takes the wider of
the two adjacent cell borders (later cell wins ties). Cells reserve the
full width of outer edges and half of inner ones. The table, not the
cells, paints row backgrounds, cell backgrounds and edges.

**Pagination.** Row groups (rows tied by row spans) are atomic. A group
that does not fit, or that lands exactly on a later page's top, moves down
by the page remainder plus the header height. The painter redraws the
header rows (backgrounds, edges, contents) at the top of each continuation
page through a shift override. At a page break the edge between two rows
closes the fragment above; the fragment below is opened by the repeated
header, or by the edge itself when there is no header. A group taller
than a page is sliced with a warning (splitting a row between its cells'
lines remains an M4 stretch goal).

## ADR-024 — Paging tolerance of 0.1 pt (M4)

**Context.** The 20k-row table dump (435 pages) reaches paged coordinates
of ~335,000 pt, where one f32 step is ~0.03 pt, so 0.01 pt comparisons
failed and some pages lost their repeated header. **Decision.** Page
decisions use a 0.1 pt tolerance (invisible). If documents grow by
another order of magnitude, paged positions move to (page, offset) pairs
or f64 in M6.

## ADR-025 — Images: passthrough JPEG, decoded PNG, sizing and orientation (M5)

**Context.** §6.6 asks for JPEG passthrough, PNG with alpha, dedupe and
`fit` modes, but leaves sizing, colour and orientation details open.

**Decision.**
- **JPEG** is never decoded. A ~100-line header reader (`images/probe.rs`)
  gets size, components (1/3/4), the Adobe APP14 marker and the EXIF
  orientation; the bytes go out as `DCTDecode`. 4-component files with an
  Adobe marker are inverted CMYK (Photoshop convention) and get
  `/Decode [1 0 1 0 1 0 1 0]`. Only 8-bit baseline/progressive frames are
  accepted; the `image` crate is built with the `png` feature only.
- **PNG** is decoded with `image` to 8-bit grey or RGB plus an alpha plane;
  a fully opaque alpha plane is dropped (no `SMask`). 16-bit samples are
  reduced to 8 bits and ICC profiles are ignored (DeviceRGB, as §4.7).
- **Compression** of image samples always uses Flate level 6, independent
  of the content-stream `compress` option: samples are compressed once,
  when an image is first decoded, and cached on the `ImageStore` entry for
  the lifetime of the store (the process-wide cache of §5.2).
- **Dedupe** is by xxh3-64 of the encoded bytes: one XObject per distinct
  image per document, numbered in hash order (deterministic).
- **Intrinsic size** is 96 dpi (1 px = 0.75 pt), as in browsers and
  react-pdf, after EXIF orientation (5–8 swap the axes). A given width or
  height determines the other through the aspect ratio. With neither, an
  image wider than a definite available width is scaled down to it,
  which matches what authors expect from `max-width: 100%` images.
- **EXIF orientation** (JPEG only) is honoured through the image matrix,
  not by re-encoding, so passthrough stays passthrough.
- **Inline data** accepts plain base64 and `data:<mime>;base64,` URLs; a
  20-line decoder avoids a dependency.
- **Limits.** Images over 50 megapixels are a validation error
  (`INVALID_IMAGE`), as are unreadable headers. Validation probes headers
  only; decoding happens at render.

## ADR-026 — Node binding: errors as JSON, local builds only (M5)

**Context.** §7 wants JS errors carrying `code`, `path` and `message`. An
`AsyncTask` can only reject with a napi `Error` (status + reason). The
user asked that nothing be published and no hosted CI run.

**Decision.** Native errors carry a JSON reason `{code, path, message}`;
`npm/index.js` (hand-written, like `index.d.ts`) turns it into an
`InkoxideError` with those properties. Engine panics are caught at the
boundary (`catch_unwind`) and become `INTERNAL` (renamed from `INTERNAL_ERROR` in ADR-042). Fonts live in a
process-wide `RwLock<FontStore>`; each render takes a snapshot (cloning
`Arc`s), so `registerFont` never waits for a running render. Renders
return a `Buffer` built from the engine's `Vec<u8>` without copying,
exposed as a plain `Uint8Array` view. The loader prefers a locally built
`npm/inkoxide.<platform>.node` (`scripts/build-node.sh`) and falls back to
the `@inkoxide/<platform>` package names of §7. The release matrix and
`npm publish` are deferred; `package.json` lists the six targets for
`@napi-rs/cli` when that time comes. `validate` ignores `UNKNOWN_IMAGE`
unless images are passed, since images are per-render options.

## ADR-027 — Word-level shaping cache (M6)

**Context.** Profiling the 100-page report showed about half of the render
time in rustybuzz. Business documents repeat the same words heavily, and
each `shape` call also rebuilt a `ShapePlan` from the font's GSUB/GPOS tables.

**Decision.** The text is split into words (runs of non-spaces) and
single spaces. Each word is shaped once per `(font, word)` and cached with
its glyphs, advances and clusters. `ShapePlan`s are cached per
`(font, direction, script)`. Both caches live in the `RustybuzzShaper`
for one render, so output stays deterministic and memory is freed with the
render. Glyph-to-Unicode mappings are recorded the first time a word is seen.

**Consequences.** Kerning and contextual substitution across a space are
lost, which is also how Blink's `CachingWordShaper` behaves. Expected
renders moved by under 0.4% of pixels, mostly around headings. Ligatures
and kerning inside words are unchanged. The report goes from about 900 ms
to about 390 ms (see BENCHMARKS.md). Scripts without spaces (CJK, Thai)
form long "words" and gain nothing, but stay correct.

## ADR-028 — Closed-form layout for simple table cells (M6)

**Context.** Each table cell was laid out as its own Taffy tree, three or
four times per cell (min-content, max-content, height at the column width,
final placement). In the 20 000-row table dump that was most of the
layout time.

**Decision.** A cell that is a default column flexbox (point padding
only; no sizes, min/max, aspect ratio or position) and whose only child is
a text node with no box of its own is laid out in closed form
(`layout/fast.rs`). The text stretches to the content width, so it is
measured at that width; the cell height is the text height plus padding
and the collapsed-border insets; `verticalAlign` shifts the text within a
taller row. Every other cell still goes through Taffy. A test lays out
the table fixtures and a synthetic table with valign, padding, borders
and ellipsis both ways and requires identical geometry.

**Consequences.** Layout of the table dump dropped from about 1.0 s to
about 0.3 s. Two code paths now have to agree; the equivalence test is the
guard, and any new layout-affecting style property must be excluded in
`simple_cell` until the fast path handles it.

## ADR-029 — Parallel paint, encoded pages, leaner parse (M6)

**Context.** §9 sets multi-thread targets and a 300 MB peak RSS for 500
pages. Display lists for every page were kept until the PDF was written,
and parsing kept serde's spare `Vec` capacity (four 584-byte nodes per
one-child cell).

**Decision.**
- Layout and pagination stay sequential. Pages are then painted in
  parallel (rayon), and each one is immediately encoded and
  Flate-compressed into an `EncodedPage`: the stream plus the fonts and
  glyphs, opacities and images it uses. The writer only assembles them.
  Object numbering does not depend on scheduling, so output is
  byte-identical whatever the thread count (checked on the table dump).
- Painting reads the text cache through `&TextCache::layout_at`. Layouts
  are `Arc`s, and the cache, shaper and hyphenator are `Sync`. A width
  missed during layout is broken on the fly without being cached.
- `RenderOptions.threads`: `None` uses rayon's global pool. `Some(n)` uses a
  dedicated pool of n threads, built once per size and kept for the
  process (`pool.rs`, capped at 256).
- Parsing first tries plain `serde_json`, and re-parses with
  `serde_path_to_error` only on failure, for the error path. Node, cell,
  row and run vectors are shrunk to fit as they are deserialized.
- Painting a table on a page scans only the rows on that page (cells and
  edges are ordered by row and found by binary search), not every row on
  every page.

**Consequences.** On the 4-vCPU dev container the table dump went from
4.4 s to 2.4 s single-threaded and 1.7 s with 4 threads, and peak RSS
from 543 MB to 290 MB. Layout (shaping) and parse are now the sequential
floor.

## ADR-030 — Shaping ahead of layout, in parallel (M6)

**Context.** After ADR-028, shaping was the largest sequential cost. It
depends only on each paragraph's runs and style, not on layout.

**Decision.** Before layout, every text node of the document (flow,
headers, footers and cell contents, in document order) is shaped in
parallel chunks. Each worker has its own `RustybuzzShaper` (word and plan
caches, glyph-to-Unicode map). Results are merged into the text cache in
chunk order. For each glyph, the Unicode text from its first occurrence in
document order wins, so ToUnicode maps, and the whole output, are identical
for any thread count. A test renders 600 mixed-font justified, hyphenated
paragraphs with 1, 3 and 8 threads and compares the bytes. Small documents
(under 128 text nodes) or a single thread shape sequentially.

**Consequences.** The 100-page report renders in about 150 ms with 4
threads. Workers do not share their word caches, so repeated words are
shaped once per worker instead of once per document; that costs a little
total CPU for a much shorter wall-clock time.

## ADR-031 — Fuzzing and memory guards (M6)

**Context.** The engine must never panic or blow up on any input (§8.6).
Hand-written tests only cover the cases someone thought of.

**Decision.**
- `fuzz/` is a cargo-fuzz crate (libFuzzer, nightly). It is not a workspace
  member, so the stable build never sees sanitizer flags. It has two
  targets that share one oracle. The oracle says: no panic; `render`
  succeeds only if `validate` is clean; a validation error from `render`
  implies `validate` reports one too; output is a complete PDF.
  - `render_json` feeds raw bytes. It is seeded with `fixtures/docs` and
    uses a dictionary of schema keys and enum values.
  - `render_tree` is structure-aware. The bytes drive a generator of
    schema-shaped documents with edge values (0, negatives, 10⁹, 10⁻⁶,
    `f32::MAX`), long words, mixed scripts, large tables and deep nesting.
    About 80 % of them validate, so most runs reach layout, pagination and
    paint. One input in eight breaks the schema on purpose.
- `scripts/fuzz.sh SECONDS` runs both targets. `scripts/ci.sh` runs a
  30-second smoke when nightly and cargo-fuzz are installed. `repro` (in
  the fuzz crate) prints the document behind a finding.
- Memory guards:
  - **Page count.** A render may produce at most `MAX_PAGES` = 100 000
    pages, checked after pagination and before anything is painted.
    Otherwise it returns `LAYOUT_ERROR`. This was the one class of finding:
    a box 10¹¹ pt tall, or `aspectRatio: 1e-6`, asked for 10⁸ pages and
    tens of GB.
  - **Nesting depth.** The JSON parser's recursion limit (128) bounds it. A
    test renders the deepest accepted document on a 2 MiB stack, with one
    and two threads.
  - **Images.** The existing 50-megapixel limit still applies.

**Consequences.** Fuzzing needs nightly, which only the developer machine
has; CI stays local (no hosted workflows). The page limit is far above
any real document (the 500-page dump uses 0.5 %). Callers who need more
can split the document.

## ADR-032 — Font subsetting and process-wide caches (M6/M7)

**Context.** v1 embedded whole font programs (ADR-017): 414 KB for a
one-page invoice with two Inter faces. The first render of a process
spent ~45 ms compressing them, missing the 50 ms cold-start target once
registration was counted. The Node binding also builds a new image store
per call, so images were decoded again on every render.

**Decision.**
- Fonts are subset with `subsetter` (Typst; named for M7 in §3, no
  default features) to the glyphs the document uses. Content streams
  keep the original glyph ids as CIDs, because pages are encoded in
  parallel before the document's glyph set is known (ADR-029). A
  Flate-compressed `CIDToGIDMap` stream maps them to the renumbered
  subset. `W` and ToUnicode stay keyed by original id.
- The subset tag (`ABCDEF+`) is derived from the font name and glyph set
  with xxh3, so output stays deterministic.
- If the subsetter rejects a font, the whole font is embedded with an
  Identity map and a warning is logged.
  `RenderOptions.subset_fonts = false` (CLI `--full-fonts`) forces this.
- Compressed subsets are cached process-wide, keyed by font hash and the
  exact glyph set, in a 32 MB LRU. Compression is most of the cost
  (miniz state setup plus ~15 KB per face), and documents from one
  template repeat their glyph sets.
- Decoded images are cached process-wide, keyed by xxh3 of the encoded
  bytes and checked for collisions, in a 64 MB LRU. Every `ImageStore`
  shares it.

**Consequences.**
- The invoice is now 29 KB instead of 414 KB.
- Cold start (registering two fonts plus the first render) takes ~8 ms
  instead of ~55 ms.
- The warm invoice render is 2.6 ms when the cache hits. A miss adds
  ~1.3 ms (subset plus compression).
- Both caches hold memory for the life of the process, bounded by their
  budgets.

## ADR-033 — MessagePack input (M7)

**Context.** §5 names MessagePack as an optional second encoding of the
same tree (`format: "msgpack"`), for callers that already hold binary
data or want to skip JSON escaping.

**Decision.**
- `rmp-serde` drives the same serde structs as JSON, so the two formats
  cannot drift. It is a normal dependency, not a cargo feature: it is
  small (rmp, rustc-hash) and a feature would double the test matrix.
- The parse mirrors the JSON one: a fast path without path tracking; on
  failure, a second pass through `serde_path_to_error` for the path and
  a version probe, so `UNSUPPORTED_VERSION` still wins over field errors.
- Errors map onto the JSON codes: semantic messages go through the same
  classifier (`MISSING_FIELD`, `UNKNOWN_FIELD`, `INVALID_VALUE`, …); a
  type mismatch at a path is `INVALID_TYPE`; malformed bytes (reserved
  marker, truncation) and trailing bytes are `PARSE_ERROR`.
- Nesting is capped at 128 (`set_max_depth`), the same as serde_json's
  recursion limit (ADR-031).
- Entry points: `InputFormat::MessagePack`; Node `format: "msgpack"`;
  CLI `--msgpack` or a `.msgpack` / `.mpk` extension.

**Consequences.** Every fixture is also rendered from MessagePack in the
test suite and must produce identical bytes; the fuzz oracle does the
same for every JSON input that renders. Maps must use string keys, as
JSON objects do.

## ADR-034 — Links and bookmarks (M7)

**Context.** §6.8 leaves the document outline and links to M7. The input
is resolved and declarative, so both must be plain properties of nodes,
added without breaking v1.

**Decision.**
- `link` (a string) on every node that has a box, and on text runs.
  `#id` targets the first displayed node with that `id` in flowing
  content, in document order; anything else must be an absolute URI with
  an RFC 3986 scheme. `javascript:` is rejected. An unknown anchor is
  `UNKNOWN_ANCHOR`. Ids stay non-unique (they were never checked); the
  first one wins.
- `bookmark: { title, level = 1 }` on the same nodes. Entries keep
  document order. Each one nests under the nearest earlier entry with a
  lower level, like HTML headings, so skipped levels need no placeholder
  entries. Bookmarks are rejected in headers and footers, because those
  repeat on every page.
- Destinations are computed after pagination from the layout tree: the
  page where the node starts and its top-left corner, written as
  `[page /XYZ left top 0]` so viewers keep the zoom. No named
  destinations are used.
- Clickable areas are display-list ops (`Op::Link`) that draw nothing.
  This keeps them in paint order and lets each page collect them while it
  is encoded in parallel. A node link covers the part of the border box
  on the page. A run link covers the run's glyphs on each line, with
  adjacent pieces merged, at line height. Annotations have no border.
  URIs are percent-encoded to 7-bit ASCII.
- The outline opens fully expanded (positive `Count`). The catalog sets
  `PageMode /UseOutlines` so viewers show it.

**Consequences.**
- Link areas ignore `rotate`: a rotated box keeps its unrotated
  rectangle.
- Links inside `overflow: hidden` are not clipped.
- Table rows and cells cannot be targets; their `id`s are only labels.
  Put the id on a node inside the cell instead.

## ADR-035 — Separate table borders (M7)

**Context.** Tables use collapsed borders: the grid resolves each shared
edge once, and the stronger border wins. Some designs need the CSS
`border-collapse: separate` model instead: spaced cards, rounded cells,
cells with their own background.

**Decision.**
- `borderCollapse: "collapse" | "separate"` (default `collapse`) and
  `borderSpacing` (default 0) on table nodes only. `borderSpacing` is
  only valid with `separate`. On any other node, cells included, both
  give `STYLE_NOT_APPLICABLE`.
- In separate mode the grid resolves no edges. Each cell's insets are
  its own border widths. The cell paints its background, rounded with
  its `borderRadius`, and then its own borders through the same code
  path as boxes, so dashed styles and radii behave as they do on boxes.
- Spacing is a single value: around the outer edge and between columns
  and rows. Columns are sized from the width left after
  `borderSpacing × (columns + 1)`. Spanning cells include the spacing
  they cover. Each row's band carries its spacing, so a page break
  between rows needs no special case. The header that repeats on a page
  starts with its own top spacing.
- Row backgrounds paint under each cell of the row, not across the
  gaps, as CSS does.

**Consequences.**
- One spacing value only. CSS's two-value form (horizontal, vertical) is
  left for later; it is additive.
- The table's own background shows through the gaps. Like any split
  node, its fragment on a page runs to the bottom of the content area.

## ADR-036 — Grid parity: areas, repeat, minmax (M7)

**Context.** v1 grids took plain track lists and numeric line placement.
Real layouts (dashboards, card walls) need what CSS grid users reach for
first: named areas, `repeat(auto-fill, minmax(…))` and implicit track
sizes. Taffy implements all of it.

**Decision.**
- Tracks stay JSON values. A string or number is a breadth as before.
  `{min, max}` is `minmax()` (an fr minimum is rejected, as CSS does), and
  `{fitContent}` is `fit-content()`. `{repeat, tracks}` is `repeat()` and
  is allowed only in the two template lists. The count is 1..=1000,
  `"auto-fill"` or `"auto-fit"`.
- Auto-repeat rules are checked up front rather than letting Taffy
  silently drop the template: one auto-repeat per list, every track in
  the list with a fixed minimum or maximum, and a positive fixed size in
  the repeated tracks. Taffy caps a grid at 10 000 tracks, which bounds
  the work for small tracks on large pages.
- `gridTemplateAreas` is the CSS string list. It is parsed by one shared
  function (validation and layout), and each area must be a rectangle.
  `gridArea: "name"` places an item via the implicit `name-start` /
  `name-end` lines. It must name an area of the parent's template and
  cannot be combined with `gridRow`/`gridColumn`. This is stricter than
  CSS, which quietly places unknown names on implicit lines.
- `gridAutoColumns`, `gridAutoRows`, `justifyItems` and `justifySelf` map
  straight to Taffy.
- Named lines (`[a] 1fr [b]`) are not offered. Areas cover the common
  case, and Taffy panics on malformed line-name lists.

**Consequences.**
- All additions are optional fields; v1 documents are unchanged.
- New `Style` fields are boxed slices and strings to keep `Node` small.

## ADR-037 — CFF OpenType fonts as CIDFontType0C (M7)

**Context.** Many fonts, Adobe's included, ship only as `.otf` files with
CFF outlines. ADR-014 rejected them.

**Decision.**
- A static font without a `glyf` table is accepted when it has a `CFF `
  table that `subsetter` can convert. Variable fonts and CFF2 are still
  rejected with `FONT_ERROR`.
- Embedding is `Type0`/`Identity-H` over a `CIDFontType0` descendant.
  The program is a bare CFF stream in `FontFile3` with
  `/Subtype /CIDFontType0C`. There is no `CIDToGIDMap`, since PDF does
  not allow one for CFF.
- `subsetter` turns every CFF font into a CID-keyed font whose charset is
  the identity (CID = new glyph id). Content streams keep the original
  glyph ids as CIDs (ADR-032), so after subsetting we patch the Top DICT
  `charset` offset (written as a fixed 5-byte integer) and append a
  format 0 charset that maps each new glyph id back to its original id.
  This needs no CFF writer of our own. The full-font path (`fullFonts`)
  runs `subsetter` over every glyph, so the identity charset is already
  correct there. Registration checks that the layout is patchable, so
  failures surface when the font is registered, not at render time.

**Consequences.**
- Shaping, metrics, ToUnicode and the rest of the text pipeline are
  unchanged. Only the program and the descendant font dictionary differ.
- The fixture `fonts-cff` mixes a TrueType and a CFF face in one
  paragraph. poppler, qpdf and pdftotext accept the output.
- If a future `subsetter` writes the charset operand in a shorter form,
  registration of CFF fonts fails loudly. The version is pinned.

## ADR-038 — SVG images as vector Form XObjects via usvg (M7)

**Context.** Logos, icons and charts usually come as SVG. Rasterising them
would lose sharpness and inflate the file. The brief allows
`resvg`/`usvg`.

**Decision.**
- `src`/`data` images whose bytes start with `<` (after an optional BOM
  and whitespace) are parsed as SVG with `usvg` 0.48.1, using
  `default-features = false`: no `text`, `fontdb`, `system-fonts` or
  `svgz`. The parse runs once per image and is cached with the decoded
  rasters (ADR-032), weighted at 8× the encoded size.
- The tree becomes one Form XObject whose BBox is the SVG's intrinsic
  size, and whose Matrix maps it onto the unit square. Fit modes,
  clipping and placement are the same as for raster images. Paths stay
  vectors, and the details are in `pdf/svg.rs`:
  - gradients become shading patterns;
  - patterns become tiling patterns;
  - opacity, blend modes, masks and complex clips become transparency
    groups and soft masks;
  - embedded JPEG/PNG reuse the raster writer.
- **No I/O.** `core` reads no files. File `href`s are ignored and only
  `data:` URLs are resolved: JPEG, PNG and nested SVG (which are checked
  again). GIF and WebP are dropped.
- **Limits for untrusted input.** These checks run on the XML before
  `usvg` expands it, because `use` and `url(#…)` references copy
  subtrees:
  - at most 1M XML nodes;
  - at most 250k elements once references are expanded, with cycles
    skipped as `usvg` does;
  - nesting at most 100 levels deep;
  - at most 100 000 px per side;
  - at most 5M path segments while writing.

  Violations are `INVALID_IMAGE` with a message.
- **Unsupported, by design for now:**
  - `<text>` with content is rejected with "convert the text to outlines".
    Silently dropping it (which `usvg` does without fonts) would lose
    content.
  - Filters: filtered elements are left out.
  - Gradient `spreadMethod` `repeat`/`reflect` is drawn as `pad`.

**Consequences.**
- The fixture `svg` covers gradients, a hatch pattern, clips (single and
  compound), a mask, group opacity, multiply blend, an embedded PNG, a
  nested SVG, dashes, even-odd fills, and the four fit modes. poppler and
  `qpdf --check` accept it.
- The `render_svg` fuzz target feeds arbitrary bytes through the image
  path with the validate/render consistency oracle (ADR-031).
- Text in SVG can be supported later by passing registered fonts to
  `usvg` (feature `text`) and embedding the glyphs as outlines.

## ADR-039 — PDF/A-2b as a render option (M7)

**Context.** Invoices and other records often have to be archived as
PDF/A. Our regular output is close to PDF/A-2b already: every font is
embedded and there is no encryption, JavaScript or external content.

**Decision.**
- PDF/A is a render option, not a document field: `RenderOptions::pdf_a`,
  `pdfA` in Node and `--pdf-a` in the CLI. The default output is
  unchanged.
- With it on, the writer adds:
  - an uncompressed XMP packet (`pdfaid` part 2, conformance B) that
    mirrors the information dictionary (title, author, subject,
    keywords, creator tool, producer, creation date in the same instant
    and offset);
  - a `GTS_PDFA1` output intent whose destination profile is an sRGB ICC
    v2 display profile;
  - a file `/ID`, the xxh3-128 of the page streams and the XMP, so output
    stays deterministic.
- The ICC profile is built in code (`pdf/pdfa.rs`): D50-adapted sRGB
  primaries and the sRGB curve sampled at 1024 points, about 2.5 KB
  before compression. There is no binary blob and no licence question.
- Metadata control characters become spaces in both the information
  dictionary and XMP, so the two stay equal after XML normalisation.
- Link annotations get the Print flag.
- `.notdef` glyphs (characters no registered font covers) are left out
  of text-showing operators, as PDF/A forbids them. The next glyph keeps
  its position.
- CMYK JPEGs cannot be passed through, since the output intent is RGB.
  They are decoded with `image` (feature `jpeg`, which is on the
  allowlist) and written as Flate RGB samples. The conversion is naive,
  like most viewers' DeviceCMYK conversion. Convert CMYK images
  upstream when colour accuracy matters.
- Transparency (opacity, SVG groups and soft masks) is allowed in
  PDF/A-2. The output intent provides the blending colour space.

**Consequences.**
- Every fixture also renders as PDF/A. `tests/fixtures.rs` checks the
  markers and determinism, and runs veraPDF's 2b profile when `VERAPDF`
  is set. All fixtures pass veraPDF 1.30. The fixture `pdfa-edge` covers
  `.notdef`, CMYK and escaped metadata.
- PDF/A-2u (Unicode mapping) is likely met already, since every font has
  a ToUnicode CMap, but it is not claimed or tested. PDF/A-2a and PDF/UA
  need tagging, which remains a non-goal.

## ADR-040 — Interned node styles (M7)

**Context.** `Style` is 560 bytes and every `Node` and `TableCell` held
one inline. Generated documents repeat a handful of styles across
thousands of nodes: the 500-page table dump (20k rows × 5 cells) reached
306 MB peak RSS after the grid fields landed, above the §9 target of
300 MB. Boxing the style (`Option<Box<Style>>`) made it worse, since
nearly every node sets one.

**Decision.**
- `Node.style` and `TableCell.style` are a `NodeStyle`, an
  `Arc<Style>` that derefs to `Style`.
- Deserialisation interns styles against a thread-local list of the 16
  most recently seen ones; an equal style shares the existing `Arc`.
  The list is bounded, so a document with many distinct styles costs
  one comparison pass per node and nothing more.
- The public shape of the schema (JSON, MessagePack, JSON Schema) is
  unchanged; this is an in-memory representation only.

**Consequences.**
- Table dump peak RSS: 306 MB → ~210 MB, with no change in time and
  byte-identical output.
- Styles are read-only after parsing. Code that needs to change a style
  builds a new one.

## ADR-041 — Lint policy: clippy pedantic, documented public API

**Context.** Before the first release the code should follow the Rust
API guidelines and the usual idioms, and keep following them without a
reviewer having to notice regressions.

**Decision.**
- `[workspace.lints]` turns on `clippy::pedantic` and
  `missing_debug_implementations` for every workspace crate;
  `scripts/ci.sh` runs clippy with `-D warnings`.
- Allowed, with the reason in `Cargo.toml`: the numeric cast lints and
  `float_cmp`, because geometry is `f32` with casts bounded by validation
  and exact comparisons are deliberate. Also allowed: lints that only
  measure size or naming (`too_many_lines`, `many_single_char_names`,
  `similar_names`, …) and `must_use_candidate`, since `#[must_use]` is
  added by hand on the public API.
- `inkoxide` (core) warns on `missing_docs`; every public item is
  documented, and fallible functions have an `# Errors` section.
- `ValidationCode` and `InkoxideError` are `#[non_exhaustive]`, so new
  codes are not breaking changes. (`RenderOptions` stayed exhaustive
  here; ADR-042 later made it `#[non_exhaustive]` with builders.)
- The crate-wide `allow(dead_code)` from M1 is gone. Schema fields that
  are accepted but unused carry an `#[expect(dead_code, reason = …)]`.
- The Node crate allows `needless_pass_by_value`, `missing_errors_doc`
  and `missing_debug_implementations`: napi exports take owned JS values,
  and their errors are documented in `npm/index.d.ts`.

## ADR-042 — API freeze for integration (post-M7)

**Context.** The client (`pdf-studio`) is about to depend on the engine
through the local npm package. Every later change to the Rust or Node
surface should be additive, so the shape has to be settled now.

**Decision.**
- **Rust.** `RenderOptions` and `InputFormat` are `#[non_exhaustive]`,
  like `ValidationCode` and `InkoxideError` (ADR-041). Other crates
  build options from `RenderOptions::default()` and `with_*` setters
  (`with_compress`, `with_compression_level`, `with_creation_date`,
  `with_threads`, `with_subset_fonts`, `with_pdf_a`); the fields stay
  public for reading and assignment. A new option is then a minor
  version, not a breaking one.
- **Node options** are the Rust options in camelCase (`compress`,
  `compressionLevel`, `creationDate`, `threads`, `subsetFonts`, `pdfA`)
  plus the per-render `format` and `images`. Defaults are documented in
  `npm/index.d.ts`. `compressionLevel` outside 0–9 is
  `INVALID_ARGUMENT`.
- **Error codes.** Every Node failure is an `InkoxideError` with `code`,
  `path` and `message`. The set is closed: the `ValidationCode` strings,
  the resource codes (`FONT_ERROR`, `IMAGE_ERROR`, `LAYOUT_ERROR`,
  `PDF_ERROR`), `INVALID_ARGUMENT` for JS values napi cannot convert
  or that are out of range, and `INTERNAL` for engine bugs, including
  caught panics. `INTERNAL_ERROR` is renamed `INTERNAL` in Rust too, so
  both surfaces print the same code. `index.d.ts` lists them as string
  literal unions (`ValidationCode`, `ErrorCode`).
- **`version()`** returns `{ engine, schema, gitSha }`. `gitSha` is the
  12-digit commit the native binary was built from (`build.rs` runs
  `git rev-parse`), or `"unknown"` outside a git checkout, so a client
  can log exactly which build produced a PDF.
- **Panic test hook.** The native module exports a hidden `__testPanic`
  that panics behind the same `catch_unwind` boundary as `render`. It
  is non-enumerable in `index.js`, absent from `index.d.ts`, and exists
  only so the Node tests can prove a panic becomes `INTERNAL` without
  crashing the process.

**Consequences.**
- Struct-literal construction of `RenderOptions` and exhaustive matches
  on `InputFormat` no longer compile outside the crate. The CLI, tests
  and fuzz targets use the builders.
- `docs/API.md` describes both surfaces side by side. Any change to one
  must update `npm/index.d.ts`, `docs/API.md` and this mapping in the
  same commit.


## ADR-043 — `@inkoxide/schema`: generated types, bundled JSON Schema

**Context.** The client builds documents in TypeScript and wants to
type-check them and validate them in the browser before a round trip
to the engine. The JSON Schema is the contract (ADR-003), so the types
should come from it, not be written a second time.

**Decision.**
- `npm-schema/` is the package `@inkoxide/schema` (1.0.0, not
  published).
  - `index.d.ts` is generated from `schema/document.v1.json` by
    `json-schema-to-typescript` 16.0.0, a pinned dev dependency of
    `scripts/schema/`, through `scripts/schema/gen-ts.sh`.
  - `document.v1.json` is a copy of the schema.
  - `index.js` is hand-written. It exports `SCHEMA_VERSION`, `schema`
    and `enginePath`.
  - Generated files are checked in. `scripts/ci.sh` regenerates them
    and fails on any diff.
- The generator normalises the schema for TypeScript only:
  - The root type is named `Document`, and every `$defs` entry keeps
    its key as its type name.
  - `Node` becomes a plain union of its variants.
  - A `$ref` with a sibling `description` is wrapped in `allOf`, so the
    named type is reused rather than copied under a new name.
  - "Exactly one of" constraints (`oneOf` of `required` clauses), such
    as `text`/`placeholder` and `src`/`data`, are dropped. TypeScript
    cannot express them without index signatures that would accept any
    property.
- `Node` in the schema gains `discriminator: { propertyName: "type" }`.
  - It is an annotation, ignored by draft 2020-12 validators, so it is
    not a breaking change.
  - With ajv's `discriminator: true`, errors are reported only for the
    node type named by `type`, at the same paths as the engine.
    `examples/node-smoke` checks this.
- `enginePath(ajvError)` converts ajv's JSON Pointer to the engine's
  path syntax. For `additionalProperties` and `required` it appends the
  property name, as the engine does. The cases where the two still
  differ are listed in `npm-schema/README.md`.

**Consequences.**
- A schema change needs `scripts/schema/gen-ts.sh`, or CI fails.
- Regenerating needs pnpm and access to the npm registry. The engine
  build does not.

## ADR-044 — Hyphenation patterns registered at runtime (post-M7)

**Context.** ADR-015 left Romanian hyphenation open. The owner chose
option (b): clients register TeX patterns at runtime. That way no
pattern file with an unclear licence enters the repository.

**Decision.**
- **`HyphenationStore`.** It is public in core, with
  `register(lang, patterns: &[u8])`, `unregister`, `contains`, `len`
  and `is_empty`. It is passed through
  `RenderOptions::with_hyphenation`, the additive way allowed by
  ADR-042. `render` and `validate` keep their signatures. Cloning is
  cheap, because parsed patterns sit behind an `Arc`.
- **Parser and trie.** They are our own, about 150 lines in
  `text/hyphen.rs`. hypher 0.1.8 builds its tries at compile time and
  exposes no builder, so it cannot load patterns at runtime.
  - The input is UTF-8 TeX, as in hyph-utf8. Every `\patterns{…}` group
    feeds a Liang trie with sorted edges and flat level storage.
    Every `\hyphenation{…}` group adds exception words (`ta-ble`), which
    win over the patterns.
  - `%` comments are stripped.
  - These are errors: TeX escapes (`^^xx`, `\`) inside patterns, two
    digits in a row, unbalanced braces, invalid UTF-8, and input
    without any group.
  - Words are matched in lowercase, and offsets are byte offsets in the
    original word.
  - `\lefthyphenmin` and `\righthyphenmin` are fixed at 2/2, the
    hyph-utf8 value for Romanian and most other languages. The existing
    "words longer than five letters" rule (ADR-015) still applies.
- **Resolution order.** Registered patterns for the paragraph's language
  come first, then hypher's built-in patterns, then nothing. Soft
  hyphens in the text are always honoured. Patterns are keyed by the
  two-letter language subtag, so `ro-RO` and `ro` share them.
- **Errors.** A malformed pattern file is `HYPHENATION_ERROR`, a new
  `InkoxideError::Hyphenation` variant. Adding it is not a breaking
  change, because the enum is `#[non_exhaustive]`.
- **Surfaces.**
  - Node: `registerHyphenationPatterns(lang, data)` and
    `unregisterHyphenationPatterns(lang)`, process-wide like fonts.
    Each render takes a snapshot.
  - CLI: `--hyphenation LANG=PATH`, repeatable.
- **Test data.** The fixture `text-hyphenate-ro-patterns` registers about
  60 hand-written patterns (`fixtures/hyphenation/`, MIT). The fixture
  test registers a pattern file only for the fixture whose name
  prefixes it, so every other fixture still renders without them.
  `hyph-ro.tex` is not in the repository. It parses and renders with
  this code (checked locally), and `docs/API.md` explains how to use
  it.

**Consequences.**
- Output stays deterministic for fixed inputs. Patterns are part of the
  inputs, like fonts.
- The pattern set a client registers decides hyphenation quality, and
  the engine neither ships nor endorses a Romanian set.


## ADR-045 — Transforms, z-index, per-corner radii, per-side border styles (v1.1)

**Context.** Parity with what React PDF authors expect from CSS: more than
`rotate`, paint order control, asymmetric corners and mixed border styles.
**Decision.**
- `transform` is a list of single-key objects applied as in CSS (the matrix
  is `origin · rotate · t₁ · t₂ … · origin⁻¹`); `translate` percentages
  refer to the node's own border box. Like rotation, a transformed node is
  atomic for pagination and its link rectangle is not transformed.
- `zIndex` orders the painting of siblings only (no stacking contexts
  across parents); equal values keep document order, so documents without
  it paint exactly as before. Layout is unaffected.
- Per-corner radii are scaled down together when adjacent radii exceed a
  side (CSS Backgrounds §5.5).
- Mixed border styles paint inside the border ring clip: solid sides as
  mitred trapezoids, dashed and dotted sides as strokes along the side's
  middle line. Corners of a dashed side are therefore not curved.
