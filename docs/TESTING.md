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
  identically from MessagePack, and passes `qpdf --check`. Each also
  renders as PDF/A-2b; with `VERAPDF` pointing at a veraPDF command
  (`scripts/ci.sh` sets it when `verapdf` is on `PATH`), every PDF/A
  output must pass veraPDF's `2b` profile.
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
- `render_svg`: raw bytes registered as an image and drawn twice; seeded
  with `fixtures/images`, with an SVG dictionary (`fuzz/render_svg.dict`).

All share one oracle: no panic; `render` succeeds only when `validate`
is clean and vice versa for validation errors; the output is a complete
PDF; a document that renders from JSON renders the same bytes from
MessagePack (`render_json` and `render_tree`).

Findings land in `fuzz/artifacts/`. To inspect one:

```sh
cd fuzz
cargo run --release --bin repro -- tree artifacts/render_tree/crash-…   # prints the document, runs the oracle
cargo run --release --bin repro -- json artifacts/render_json/crash-…
cargo run --release --bin repro -- image artifacts/render_svg/crash-…
cargo run --release --bin repro -- stats corpus/render_tree             # generator validity rate
```

`scripts/ci.sh` runs a 30-second smoke per target when nightly and
cargo-fuzz are installed and skips it otherwise.

## macOS (Apple Silicon)

The engine has been verified on Linux x86_64 only; the development
container is Linux. To verify macOS arm64, run on the Mac:

```sh
# Tools (once)
xcode-select --install                       # C toolchain / linker
curl https://sh.rustup.rs -sSf | sh          # rustup; rust-toolchain.toml pins 1.97.0
brew install qpdf poppler python node pnpm   # qpdf, pdftoppm/pdfinfo, python3, node ≥ 18
python3 -m pip install --user jsonschema numpy pillow
cargo install cargo-deny                     # optional; ci.sh skips it when missing
rustup toolchain install nightly && cargo install cargo-fuzz   # optional (fuzz smoke)
brew install verapdf                         # optional (PDF/A validation); or the veraPDF installer

# Checks
scripts/ci.sh
```

Expected output: the first line is `platform: Darwin arm64`, every step
prints its `== …` header, the Node step prints `# fail 0`, the visual
step lists `ok   <fixture>` for every fixture, and the run ends with
`== all checks passed` (exit status 0).

Notes:

- The Node binding is built by `scripts/build-node.sh`, which runs
  `cargo build -p inkoxide-node` and copies `libinkoxide_node.dylib` to
  `npm/inkoxide.darwin-arm64.node`. `napi-build` sets the macOS linker
  flags (`-undefined dynamic_lookup`), so `@napi-rs/cli` is not needed.
- PDFs are byte-identical across platforms (the engine does no
  platform-dependent arithmetic on the output path), but poppler and
  FreeType from Homebrew may antialias slightly differently from the
  Linux build that produced `fixtures/expected/`. If the visual step
  fails while `cargo test` passes, compare `target/visual/*-diff.png`; for
  pure antialiasing noise, rerun with a looser comparison, e.g.
  `INKOXIDE_VISUAL_THRESHOLD=0.003 scripts/ci.sh`, and record the value
  needed in this section. Do not regenerate the references on macOS.
- To confirm cross-platform byte identity, compare
  `shasum -a 256 target/fixtures/*.pdf` from both machines.
