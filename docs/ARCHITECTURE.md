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
| Fonts | `fonts` | registry only (M2) |
| Text | `text` | M2 |
| Layout | `layout` | M1 |
| Pagination | `paginate` | M3 |
| Paint | `paint` (display list types) | types only (M1) |
| PDF | `pdf` | M0 ✅: pages, page tree, content streams, Info |
| Images | `images` | registry only (M5) |

## Public API (core)

```rust
pub fn validate(doc: &[u8], format: InputFormat, fonts: &FontStore, images: &ImageStore) -> Vec<ValidationError>;
pub fn render(doc: &[u8], format: InputFormat, options: &RenderOptions, fonts: &FontStore, images: &ImageStore)
    -> Result<Vec<u8>, InkoxideError>;
```

Everything else is `pub(crate)`.

## Testing

- Unit tests next to each module (`validate/tests.rs`, `date.rs`, …).
- `crates/core/tests/fixtures.rs`: every `fixtures/docs/*.json` validates,
  renders, renders byte-identically twice and passes `qpdf --check`.
- `crates/core/tests/schema_parity.rs`: serde structs accept exactly the
  properties the JSON Schema declares.
- `scripts/check-schema.py`: fixtures conform to the JSON Schema.
- `scripts/ci.sh`: runs all of the above plus fmt, clippy and cargo-deny.
