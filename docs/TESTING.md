# Testing

Everything runs locally; there are no hosted CI workflows.

```sh
scripts/ci.sh          # fmt, clippy, tests, schema, qpdf, visual diff, cargo-deny, node, fuzz smoke
scripts/ci.sh --quick  # fmt + tests + schema
```

## Suites

- Unit tests next to each module (`validate/tests.rs`, `lru.rs`, …).
- `crates/core/tests/fixtures.rs`: every `fixtures/docs/*.json`
  validates, renders byte-identically twice (and with one thread), renders
  identically from MessagePack, and passes `qpdf --check`.
- `crates/core/tests/limits.rs`: memory guards (page limit, deepest
  accepted nesting on a small stack).
- `crates/core/tests/schema_parity.rs`: serde structs accept exactly the
  properties the JSON Schema declares.
- `scripts/check-schema.py`: fixtures conform to the JSON Schema.
- `scripts/visual-diff.py`: rasterises fixture PDFs and compares them with
  `fixtures/expected/*.png`; `scripts/update-expected.sh` regenerates the
  references for review.
- `npm/test`: the Node binding (`scripts/build-node.sh` first).
- `cargo bench -p inkoxide`: criterion benchmarks (docs/BENCHMARKS.md).

## Fuzzing

Needs the nightly toolchain and cargo-fuzz:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
scripts/fuzz.sh 60       # both targets, 60 s each
scripts/fuzz.sh 3600     # the M6 acceptance run
```

Targets (in `fuzz/`, outside the workspace):

- `render_json`: raw bytes, seeded with `fixtures/docs`, with a dictionary
  of schema keys (`fuzz/render_json.dict`).
- `render_tree`: a structure-aware generator of schema-shaped documents
  (edge values, long words, mixed scripts, big tables, deep nesting).

Both share one oracle: no panic; `render` succeeds only when `validate`
is clean and vice versa for validation errors; the output is a complete
PDF; a document that renders from JSON renders the same bytes from
MessagePack.

Findings land in `fuzz/artifacts/`. To inspect one:

```sh
cd fuzz
cargo run --release --bin repro -- tree artifacts/render_tree/crash-…   # prints the document, runs the oracle
cargo run --release --bin repro -- json artifacts/render_json/crash-…
cargo run --release --bin repro -- stats corpus/render_tree             # generator validity rate
```

`scripts/ci.sh` runs a 30-second smoke per target when nightly and
cargo-fuzz are installed and skips it otherwise.
