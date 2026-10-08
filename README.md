# Inkoxide

A native PDF layout and rendering engine in Rust. It turns a declarative,
fully resolved document tree (boxes, text, images, tables) into a PDF, with
pagination done properly: tables that split with repeated headers,
keep-together blocks, headers and footers, page numbers.

```
render(document_tree, options) -> pdf_bytes
```

> Status: **M7 (feature-complete for v1, unpublished)**. Schema,
> validation (JSON and MessagePack), flex/grid layout, box painting, text
> (shaping, wrapping, justification, hyphenation, fallback, selectable
> output), pagination (splitting, keep-together, forced breaks,
> headers/footers, page numbers), tables (spans, collapsed borders, row
> splitting with repeated headers), images (JPEG passthrough, PNG with
> alpha, SVG as vector graphics, fit modes), font subsetting (TrueType and
> CFF), links and bookmarks, PDF/A-2b output, parallel rendering, fuzzing,
> and the napi-rs binding all work. Nothing is published to npm or
> crates.io yet. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
> [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Supported platforms

| Platform | Status |
|---|---|
| Linux x86_64 (glibc) | ✅ verified: `scripts/ci.sh` green |
| macOS arm64 | ⏳ not yet verified; steps in [docs/TESTING.md](docs/TESTING.md#macos-apple-silicon) |
| Linux musl, Linux arm64, macOS x86_64, Windows | untested |

## Quick start

```sh
cargo run -p inkoxide-cli -- validate fixtures/docs/empty.json --fonts fixtures/fonts
cargo run -p inkoxide-cli -- render fixtures/docs/empty.json --fonts fixtures/fonts -o out.pdf
```

## Node.js

```sh
scripts/build-node.sh        # builds npm/inkoxide.<platform>.node
```

```js
import { registerFont, render } from 'inkoxide'; // or require('./npm')

registerFont('Inter-Regular', fontBytes);           // process-wide
const pdf = await render(docJson, { images: { logo: pngBytes } });
```

The API is in [npm/index.d.ts](npm/index.d.ts). Errors are
`InkoxideError`s with `code`, `path` and `message`.

## Layout

- `schema/document.v1.json`: the input contract (JSON Schema 2020-12),
  documented in [docs/SCHEMA.md](docs/SCHEMA.md)
- `crates/core`: the engine (crate `inkoxide`)
- `crates/cli`: the `inkoxide` command-line tool
- `crates/node`: napi-rs binding; `npm/`: the JS package around it
- `fixtures/`: input documents, OFL fonts, images, expected renders
- `docs/DECISIONS.md`: architecture decision log
- `docs/DEPENDENCY_WATCH.md`: the pending rustybuzz/ttf-parser swap (`scripts/dep-watch.sh`)

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
