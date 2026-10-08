# Architecture

Inkoxide is a pure function `render(document_tree, options) -> pdf_bytes`.
This file tracks the implementation; the full design rationale is the build
brief, and individual decisions are in [DECISIONS.md](DECISIONS.md).

## Crates

| Crate | Path | Role |
|---|---|---|
| `inkoxide` | `crates/core` | The engine. `#![forbid(unsafe_code)]`, no I/O, no napi. |
| `inkoxide-cli` | `crates/cli` | `inkoxide render / validate / bench` — dev and CI tool; does the file I/O. |
| `inkoxide-node` | `crates/node` | napi-rs binding → npm `inkoxide` (M5). |
| `inkoxide-fuzz` | `fuzz` | cargo-fuzz targets (nightly, outside the workspace). |

## Pipeline

```
bytes ──serde──▶ schema::Document ──validate──▶ (arena tree) ──layout──▶ boxes
      ──paginate──▶ pages ──paint──▶ display lists ──pdf──▶ bytes
```

| Stage | Module | Status |
|---|---|---|
| Parse | `schema` (+ `schema/de.rs`); JSON or MessagePack (`validate::parse`) | M0 ✅, MessagePack M7 ✅ |
| Validate | `validate` | M0 ✅ |
| Fonts | `fonts` (registry, metrics, `FontSet`) + `pdf/font.rs` (Type0/CID embedding, subsetting, ToUnicode) | M2 ✅, subsetting M7 ✅ |
| Text | `text` (rustybuzz shaping, UAX #14 breaking, hyphenation, fallback, justify, ellipsis) | M2 ✅ |
| Layout | `layout` (arena tree + Taffy) | M1 ✅ |
| Tables | `layout/table.rs` (columns, rows, spans, collapsed borders) + table splitting and header repeat | M4 ✅ |
| Pagination | `paginate` (shift accumulation, head rule, atomic nodes, forced breaks) + `pipeline` | M3 ✅ |
| Paint | `paint` (boxes: background, borders, radius, opacity, rotate, clip) | M1 ✅ |
| PDF | `pdf` | M0 ✅: pages, page tree, content streams (encoded in parallel), Info; links and outline M7 ✅ |
| Images | `images` (JPEG passthrough, PNG + SMask, SVG as vector Form XObjects via `usvg`, fit modes, process-wide decode cache) | M5 ✅, SVG M7 ✅ |

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

`pipeline::run` lays out every page template (flow, header, footer),
paginates each flow, then paints page by page: background → flow (clipped
to the content band, only nodes and lines on the page) → fixed nodes →
header → footer. `FlowPart` tells the box painter which part of a flow to
paint; leaf painters get a `ContentCtx` with the node's shift, per-line
shifts and the page window. See ADR-021/022.

## Testing

See [TESTING.md](TESTING.md): unit and fixture tests (byte-determinism,
JSON/MessagePack parity, `qpdf --check`), schema parity, visual
regression, the Node suite, benchmarks and fuzzing.
