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
| `packages/tailwind` | Tailwind class strings to engine styles, through the Tailwind v4 compiler: `@inkoxide/tailwind`. |
| `packages/react` | React components and a reconciler that build document trees: `@inkoxide/react`. |
| `packages/render` | `renderToBuffer`, `renderToFile`, `Font.register` and the bundled fonts: `@inkoxide/render`. |
| `examples` | Example documents in React; they produce the screenshots in the READMEs. |
| `benchmarks` | Comparisons with other PDF libraries. |
| `fixtures` | Engine test documents, fonts, images and reference renders. |
| `fuzz` | cargo-fuzz targets (nightly, outside the Cargo workspace). |

The npm packages depend on each other in this order:
`@inkoxide/schema` → `@inkoxide/tailwind` → `@inkoxide/react` → `@inkoxide/render`,
which also depends on `inkoxide`.

## From React to a document tree

`@inkoxide/react` is a React reconciler whose host components (`Document`,
`Page`, `View`, `Text`, `Image`, `Table`, …) build an in-memory tree. After
React commits, the tree is converted into a schema v1 document: `className`
strings go through `@inkoxide/tailwind` (the real Tailwind compiler turns
classes into CSS, and the CSS declarations are mapped to engine styles),
style objects are normalised, and fonts are resolved against the font
registry with CSS font matching. In development the tree is validated
against the JSON Schema, and errors carry the component stack.

`@inkoxide/render` registers the bundled fonts with the engine once per
process, renders the tree, and renders a second time when the document
uses `render` props that need the page count.

## Pipeline

```
bytes ──serde──▶ schema::Document ──validate──▶ (arena tree) ──layout──▶ boxes
      ──paginate──▶ pages ──paint──▶ display lists ──pdf──▶ bytes
```

| Stage | Module | What it does |
|---|---|---|
| Parse | `schema`, `schema/de.rs` | JSON or MessagePack into typed structs, with the path of every error |
| Validate | `validate` | Semantic checks: font ids, image ids, anchors, value ranges |
| Fonts | `fonts`, `pdf/font.rs` | Registry and metrics, WOFF/WOFF2 decoding; Type0/CID embedding of TrueType and CFF, subsetting, ToUnicode |
| Text | `text` | rustybuzz shaping, UAX #14 line breaking, hyphenation, font fallback, bidi, justification |
| Layout | `layout` | An arena tree laid out by Taffy (flexbox and grid), with a fast path for simple table cells |
| Tables | `layout/table.rs` | Column widths, spans, collapsed borders; splitting with repeated header rows |
| Pagination | `paginate`, `pipeline` | Breaks flows into pages: orphans and widows, keep-together, forced breaks |
| Paint | `paint` | Display lists: backgrounds, borders, radii, opacity, transforms, clipping |
| PDF | `pdf` | Pages and content streams (encoded in parallel), links, outline, forms, PDF/A-2b |
| Images | `images` | JPEG passthrough, PNG with SMask, GIF and WebP, SVG as vector form XObjects |

## Public API (core)

```rust
pub fn validate(doc: &[u8], format: InputFormat, fonts: &FontStore, images: &ImageStore) -> Vec<ValidationError>;
pub fn render(doc: &[u8], format: InputFormat, options: &RenderOptions, fonts: &FontStore, images: &ImageStore)
    -> Result<Vec<u8>, InkoxideError>;
```

Everything else is `pub(crate)`.

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

`TextCache` is the layout engine's `LeafMeasure` for text nodes: Taffy asks
for min-content / max-content / definite widths and gets the paragraph's
size. Glyph → Unicode mappings are collected while shaping and become the
ToUnicode CMaps, so text is selectable and searchable.

## Pagination and paint

`pipeline::run` lays out every page template (flow, header, footer,
overlays), paginates each flow, then paints page by page: background →
flow (clipped to the content band, only nodes and lines on the page) →
fixed nodes → overlays → header → footer. An overlay is a page child that
is fixed and absolutely positioned: it is laid out on its own against the
page content box, like a band, instead of in the flow. `FlowPart` tells
the box painter which part of a flow to paint; leaf painters get a
`ContentCtx` with the node's shift, per-line shifts and the page window.
Elements with `debug` have their boxes recorded as they are painted
(`paint/debug.rs`); the outlines are drawn last, over everything, the
flow's clipped to its content band.

Page-number placeholders are laid out as `999` until the page count is
known. Flow text keeps that width, because pagination depends on it. A
header, footer or overlay that shows page numbers is laid out again on each page,
in the page's paint job: `text::PageText` puts the page's digits in place
of the stand-ins (`fill_numbers`) and breaks those paragraphs' lines
again, and reads every other paragraph from the shared cache.

## Testing

See [TESTING.md](TESTING.md): unit and fixture tests (byte-determinism,
JSON/MessagePack parity, `qpdf --check`), schema parity, visual
regression, the Node suite, benchmarks and fuzzing.
