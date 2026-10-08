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

## Pipeline

```
bytes ──serde──▶ schema::Document ──validate──▶ (arena tree) ──layout──▶ boxes
      ──paginate──▶ pages ──paint──▶ display lists ──pdf──▶ bytes
```

| Stage | Module | Status |
|---|---|---|
| Parse | `schema` (+ `schema/de.rs`) | M0 ✅ |
| Validate | `validate` | M0 ✅ |
| Fonts | `fonts` (registry, metrics, `FontSet`) + `pdf/font.rs` (Type0/CID embedding, ToUnicode) | M2 ✅ |
| Text | `text` (rustybuzz shaping, UAX #14 breaking, hyphenation, fallback, justify, ellipsis) | M2 ✅ |
| Layout | `layout` (arena tree + Taffy) | M1 ✅ |
| Pagination | `paginate` | M3 |
| Paint | `paint` (boxes: background, borders, radius, opacity, rotate, clip) | M1 ✅ |
| PDF | `pdf` | M0 ✅: pages, page tree, content streams, Info |
| Images | `images` | registry only (M5) |

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

## Testing

- Unit tests next to each module (`validate/tests.rs`, `date.rs`, …).
- `crates/core/tests/fixtures.rs`: every `fixtures/docs/*.json` validates,
  renders, renders byte-identically twice and passes `qpdf --check`.
- `crates/core/tests/schema_parity.rs`: serde structs accept exactly the
  properties the JSON Schema declares.
- `scripts/check-schema.py`: fixtures conform to the JSON Schema.
- `scripts/visual-diff.py`: rasterises fixture PDFs (`pdftoppm -r 72`) and
  compares them with `fixtures/expected/*.png` (≤ 0.1 % differing pixels);
  `scripts/update-expected.sh` regenerates the references for review.
- `cargo bench -p inkoxide`: criterion benchmark of every fixture.
- `scripts/ci.sh`: runs all of the above plus fmt, clippy and cargo-deny.
