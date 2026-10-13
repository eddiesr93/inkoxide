# Architecture

Inkoxide is a pure function `render(document_tree, options) -> pdf_bytes`,
with a React front end that builds the document tree. This page describes
how the pieces fit together, for contributors.

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | The engine, crate `inkoxide`. `#![forbid(unsafe_code)]`, no I/O, no napi. |
| `crates/cli` | `inkoxide render / validate / bench`, a development tool that does the file I/O. |
| `crates/node` | The napi-rs binding, published as the npm package `inkoxide`. |
| `packages/inkoxide` | The npm package for the binding: loader, types and tests. |
| `packages/schema` | The document schema (`document.v1.json`), its TypeScript types and changelog: `@inkoxide/schema`. |
| `packages/style` | The flat style object, CSS units, and the readers of lengths, keywords, colours, `flex`, transforms and positions that React and Tailwind share: `@inkoxide/style`. |
| `packages/tailwind` | Tailwind class strings to engine styles, through the Tailwind v4 compiler: `@inkoxide/tailwind`, optional. |
| `packages/react` | React components and a reconciler that build document trees: `@inkoxide/react`. |
| `packages/render` | `renderToBuffer`, `renderToFile`, `Font.register` and the bundled fonts: `@inkoxide/render`; templates served by id: `@inkoxide/render/templates`. |
| `examples` | Example documents in React; they produce the screenshots in the READMEs. |
| `benchmarks` | Comparisons with other PDF libraries. |
| `fixtures` | Engine test documents, fonts, images and reference renders. |
| `fuzz` | cargo-fuzz targets (nightly, outside the Cargo workspace). |

The npm packages depend on each other in this order:
`@inkoxide/schema` → `@inkoxide/style` → `@inkoxide/react` → `@inkoxide/render`,
which also depends on `inkoxide`. `@inkoxide/tailwind` depends on `@inkoxide/style`, and
`@inkoxide/react` and `@inkoxide/render` load it only when it is installed (an optional peer
dependency), for `className`.

## From React to a document tree

`@inkoxide/react` is a React reconciler whose host components (`Document`,
`Page`, `View`, `Text`, `Image`, `Table`, …) build an in-memory tree. After
React commits, the tree is converted into a schema v1 document: `className`
strings go through `@inkoxide/tailwind` when it is installed (the real
Tailwind compiler turns classes into CSS, and the CSS declarations are
mapped to engine styles), style objects are read with the same readers of
lengths, keywords, colours, `flex`, transforms and positions from
`@inkoxide/style` (`em`, `vw` and `vh` resolve against the element's font
size and the page in both), and fonts are resolved against the font
registry with CSS font matching. In development the tree is validated
against the JSON Schema, and errors carry the component stack.

`@inkoxide/render` registers the bundled fonts with the engine once per
process, renders the tree, and, when the document uses `render` props that
need the page count, renders again with the page count until it settles
(at most four renders).

## Pipeline

```
bytes ──serde──▶ schema::Document ──validate──▶ (arena tree) ──layout──▶ boxes
      ──paginate──▶ pages ──paint──▶ display lists ──pdf──▶ bytes
```

| Stage | Module | What it does |
|---|---|---|
| Parse | `validate` (`parse`), `schema` | JSON or MessagePack into the typed structs of `schema` (hand-written deserialisers in `schema/de.rs`); serde errors become validation errors with their path and code |
| Validate | `validate` (`check`) | Semantic checks: font ids, image ids (images must decode), anchors, value ranges |
| Fonts | `fonts`, `pdf/font.rs` | Registry and metrics, WOFF/WOFF2 decoding, subsetting, CFF programs (`fonts/cff.rs`); Type0/CID embedding of TrueType and CFF, ToUnicode |
| Text | `text` | rustybuzz shaping, UAX #14 line breaking, hyphenation, font fallback, bidi, justification |
| Layout | `layout` | An arena tree laid out by Taffy (flexbox and grid), with a fast path for simple table cells |
| Tables | `layout/table.rs` | Column widths, spans, collapsed borders; splitting with repeated header rows |
| Pagination | `paginate`, `pipeline` | Breaks flows into pages: orphans and widows, keep-together, forced breaks |
| Paint | `paint` | Display lists: backgrounds, borders, radii, opacity, transforms, clipping |
| PDF | `pdf` | Pages and content streams (encoded in parallel), links, outline, forms, PDF/A-2b |
| Images | `images` | JPEG passthrough, PNG with SMask, GIF and WebP, SVG as vector form XObjects, SVG text read and replaced with probes (`images/svg_text`) |

## Public API (core)

```rust
pub fn validate(doc: &[u8], format: InputFormat, fonts: &FontStore, images: &ImageStore) -> Vec<ValidationError>;
pub fn validate_without_images(doc: &[u8], format: InputFormat, fonts: &FontStore) -> Vec<ValidationError>;
pub fn render(doc: &[u8], format: InputFormat, options: &RenderOptions, fonts: &FontStore, images: &ImageStore)
    -> Result<Vec<u8>, InkoxideError>;
```

Besides these, the crate exports only their inputs and outputs (`RenderOptions`,
`InputFormat`, `FontStore`, `ImageStore`, `HyphenationStore`, `InkoxideError`,
`ValidationError`, `ValidationCode`) and the constants `ENGINE_VERSION` and
`SCHEMA_VERSION`. Every module is private, so nothing else is reachable from
outside the crate.

## Layout data model

`layout::Tree` is an arena (`Vec<LNode>`) in breadth-first order, so each
node's children are a contiguous index range. Index 0 is a synthetic root:
the page content box (column flex, fixed width, unbounded height). After
`LayoutEngine::compute` every node holds absolute `Geometry` (border box,
plus resolved border and padding) in flow coordinates; paint adds the page
margin offset.

## Text pipeline

```
runs ─▶ prepare (whitespace, soft hyphens, font per char, rustybuzz) ─▶ Prepared   (cached per node)
Prepared + width ─▶ break_lines (greedy, metrics, maxLines/ellipsis) ─▶ LineLayout (cached per width)
LineLayout ─▶ TextPainter (align, justify, placeholders, decorations) ─▶ Op::Text
```

Two third-party crates stay behind one module each, so replacing them
touches only that module: ttf-parser (font parsing) is used only in
`fonts`, which gives the rest of the engine metrics, glyph ids and
advances; rustybuzz (shaping) only in `text/prepare.rs`, which parses its
shaping faces from the font bytes once per render and shares them between
threads. `pdf` depends on `fonts` and `images`, never the reverse: `fonts`
subsets font programs and caches the compressed whole program, and the PDF
writer supplies the compression; the writer and `images` both compress
through `flate.rs`.

SVG text goes through the same shaper. `images/svg_text` reads each
`<text>` of an SVG once, when the SVG is decoded: the cascade of its text
properties (simplecss and svgtypes are used only in its `extract.rs`, at
the versions usvg uses), white space and character positions. Each render
lays the text out with its fonts before layout (`text/svg.rs`, one
`TextShaper::prepare` call per chunk, so fallback fonts, bidi, the glyph
limit and ToUnicode apply), replaces each `<text>` with a "probe" path
over its ink, and converts the SVG again (cached by the rewritten source).
usvg resolves the probes' paint; the PDF writer draws each probe's text in
its place as PDF text (`pdf/svg.rs`). See
[ADR 0001](adr/0001-svg-text-probes.md).

`TextCache` is the layout engine's `LeafMeasure` for text nodes: Taffy asks
for min-content / max-content / definite widths and gets the paragraph's
size. Glyph → Unicode mappings are collected while shaping and become the
ToUnicode CMaps, so text is selectable and searchable.

## Pagination and paint

`pipeline::run` lays out every page template (flow, header, footer,
overlays), paginates each flow, then paints page by page: background →
overlays with a negative `zIndex` → flow (clipped to the content band,
only nodes and lines on the page) → fixed nodes → the other overlays →
header → footer. An overlay is a page child that is fixed and absolutely
positioned: it is laid out on its own against the page content box, like
a band, instead of in the flow. Overlays are painted by `zIndex`, then in
document order. `FlowPart` tells
the box painter which part of a flow to paint; leaf painters get a
`ContentCtx` with the node's shift, per-line shifts and the page window.
Elements with `debug` have their boxes recorded as they are painted
(`paint/debug.rs`); the outlines are drawn last, over everything, the
flow's clipped to its content band.

Page-number placeholders are laid out as `999` until the page count is
known. Then flow text that shows them gets the numbers of the page its
first number lands on (`TextCache::set_numbers`, through `fill_numbers`),
and the flows with such text are laid out and paginated again, until no
number changes (at most three times; `pipeline::number_flows`). Text in a
fixed node or a table's header rows is painted on several pages and keeps
the stand-ins' width. The painter always draws the page's own digits. A
header, footer or overlay that shows page numbers is laid out again on each page,
in the page's paint job: `text::PageText` puts the page's digits in place
of the stand-ins and breaks those paragraphs' lines again, and reads every
other paragraph from the shared cache.

## Testing

See [TESTING.md](TESTING.md): unit and fixture tests (byte-determinism,
JSON/MessagePack parity, `qpdf --check`), schema parity, visual
regression, the Node suite, benchmarks and fuzzing.
