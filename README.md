# Inkoxide

A native PDF layout and rendering engine in Rust. It turns a declarative,
fully resolved document tree (boxes, text, images, tables) into a PDF, with
pagination done properly: tables that split with repeated headers,
keep-together blocks, headers and footers, page numbers.

```
render(document_tree, options) -> pdf_bytes
```

> Status: **M2 (text & fonts)**. Schema, validation, flex/grid layout, box
> painting, and text (shaping, wrapping, justification, hyphenation, font
> fallback, selectable/searchable output) work. Pagination lands in M3. See
> [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Quick start

```sh
cargo run -p inkoxide-cli -- validate fixtures/docs/empty.json --fonts fixtures/fonts
cargo run -p inkoxide-cli -- render fixtures/docs/empty.json --fonts fixtures/fonts -o out.pdf
```

## Layout

- `schema/document.v1.json`: the input contract (JSON Schema 2020-12),
  documented in [docs/SCHEMA.md](docs/SCHEMA.md)
- `crates/core`: the engine (crate `inkoxide`)
- `crates/cli`: the `inkoxide` command-line tool
- `fixtures/`: input documents, OFL fonts, images, expected renders
- `docs/DECISIONS.md`: architecture decision log

## Development

```sh
scripts/ci.sh          # everything: fmt, clippy, tests, schema, qpdf, cargo-deny
scripts/ci.sh --quick  # fmt + tests + schema
```

Needs Python 3 with `jsonschema` for the schema checks and, optionally,
`qpdf`, `pdftoppm` (poppler) and `cargo-deny`.

## Licence

MIT. Fixture fonts are under the SIL Open Font License (see
`fixtures/fonts/*-OFL.txt`).
