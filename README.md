# Inkoxide

A native PDF layout and rendering engine in Rust, with a Node.js binding.
You give it a declarative, fully resolved document tree (boxes, text,
images, tables) as JSON or MessagePack, and it returns PDF bytes. It
paginates properly: tables split with repeated headers, blocks stay
together, headers and footers repeat, and page numbers are filled in.

```
render(document_tree, options) -> pdf_bytes
```

A 98-page report with 40 tables renders in about 130 ms on 4 cores
([docs/BENCHMARKS.md](docs/BENCHMARKS.md)).

## Status

**v1 is feature-complete and ready for integration. Nothing is published**
to npm or crates.io; clients install the packages from a local checkout.
The public API is frozen (ADR-042), so changes from now on are additive.

What works:

- **Input:** schema validation of JSON and MessagePack, with error paths.
- **Layout:** flex and grid layout.
- **Text:** shaping, wrapping, justification, hyphenation (including
  runtime TeX patterns), font fallback, and selectable text.
- **Pagination:** splitting, keep-together, forced breaks, headers and
  footers, page numbers.
- **Tables:** spans, collapsed or separate borders, and repeated header
  rows.
- **Images:** JPEG passthrough, PNG with alpha, SVG as vectors.
- **Output:** font subsetting (TrueType and CFF), links and bookmarks,
  PDF/A-2b, and parallel rendering.

The output is deterministic for a fixed creation date.

## Platforms

| Platform | Status |
|---|---|
| Linux x86_64 (glibc) | ✅ verified: `scripts/ci.sh` green |
| macOS arm64 | ⏳ not yet verified; steps in [docs/TESTING.md](docs/TESTING.md#macos-apple-silicon) |
| Linux musl, Linux arm64, macOS x86_64, Windows | untested |

## Setup

You need Rust (rustup picks up 1.97.0 from `rust-toolchain.toml`),
Node.js 22 or later, pnpm, Python 3 with `jsonschema`, `numpy` and
`Pillow`, and poppler (`pdftoppm`) for the visual diff. Optionally,
install `qpdf`, `cargo-deny`, and nightly Rust with `cargo-fuzz`; `ci.sh`
skips those checks when the tools are missing.

```sh
git clone https://github.com/eddiesr93/inkoxide && cd inkoxide
scripts/ci.sh                                          # build and test everything
scripts/build-node.sh                                  # build the Node package in npm/
(cd examples/node-smoke && pnpm install && pnpm test)  # use it like a client app
```

`examples/node-smoke` installs `inkoxide` and `@inkoxide/schema` with
`file:` dependencies, exactly as a client would.

## Documentation

- [docs/CLIENT_INTEGRATION.md](docs/CLIENT_INTEGRATION.md): how to add
  Inkoxide to a Node app (fonts, images, errors, threading, determinism,
  hyphenation).
- [docs/API.md](docs/API.md): the Rust and Node APIs side by side, with
  options and error codes.
- [schema/EXAMPLES.md](schema/EXAMPLES.md): complete example documents.
  [docs/SCHEMA.md](docs/SCHEMA.md) is the full schema reference.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
  [docs/DECISIONS.md](docs/DECISIONS.md): design and the decision log.
- [docs/TESTING.md](docs/TESTING.md): test suites, fixtures and fuzzing.
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md): performance numbers.
- [docs/DEPENDENCY_WATCH.md](docs/DEPENDENCY_WATCH.md): the pending
  rustybuzz/ttf-parser swap (`scripts/dep-watch.sh`).

## Command line

```sh
cargo run --release -p inkoxide-cli -- validate fixtures/docs/invoice.json --fonts fixtures/fonts
cargo run --release -p inkoxide-cli -- render fixtures/docs/invoice.json --fonts fixtures/fonts -o invoice.pdf
```

## Repository layout

- `schema/document.v1.json`: the input contract (JSON Schema 2020-12).
- `crates/core`: the engine (crate `inkoxide`).
- `crates/cli`: the `inkoxide` command-line tool.
- `crates/node`: the napi-rs binding. `npm/` is the JS package around it.
- `npm-schema/`: `@inkoxide/schema`, with TypeScript types and the
  bundled JSON Schema.
- `fixtures/`: input documents, OFL fonts, images and expected renders.
- `examples/node-smoke`: a client-style smoke test.

## Development

```sh
scripts/ci.sh          # everything: fmt, clippy, tests, Node, schema, qpdf, cargo-deny, fuzz smoke, visual diff
scripts/ci.sh --quick  # fmt + tests + schema
scripts/bench-headline.sh  # headline benchmark (median of 10)
```

## Licence

MIT. The fixture fonts are under the SIL Open Font License (see
`fixtures/fonts/*-OFL.txt`).
