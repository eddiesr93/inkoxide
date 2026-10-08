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
(`FontFile3`) is M7. Most web fonts ship TrueType builds.

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
boundary (`catch_unwind`) and become `INTERNAL_ERROR`. Fonts live in a
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
