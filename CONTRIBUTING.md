# Contributing

Thank you for helping. Bug reports, fixes, documentation and new features are all welcome. For
anything larger than a small fix, please open an issue first, so we can agree on the approach
before you spend time on it.

## Setting up

You need:

- Rust: rustup installs the pinned version from `rust-toolchain.toml`;
- Node.js 22 or later, and pnpm (`corepack enable` installs the pinned version);
- Python 3 with `jsonschema`, for the schema checks.

```sh
git clone https://github.com/eddiesr93/inkoxide && cd inkoxide
scripts/build-node.sh    # build the native binding into packages/inkoxide
pnpm install
```

Optional, for the full local check: poppler and `numpy`/`Pillow` (visual regression), `qpdf`
(PDF structure), `cargo-deny`, and the nightly toolchain `scripts/fuzz.sh --toolchain` prints,
with `cargo-fuzz` (fuzzing).

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the repository is organised and how
a document goes from React to PDF.

## Making a change

```sh
cargo test --workspace     # the engine
pnpm test                  # the TypeScript packages
```

`scripts/ci.sh` runs every check a pull request must pass, and the slower ones;
[docs/TESTING.md](docs/TESTING.md) describes it and each test suite.

- **Engine changes** need a test: a unit test next to the code, or a fixture in
  `fixtures/docs/` when the change affects output.
- **Output changes** show up in the visual regression test. Check `target/visual/*-diff.png`,
  and when the new output is right, regenerate the references with
  `scripts/update-expected.sh` (on Linux) and commit them with the change.
- **Schema changes** start in `packages/schema/document.v1.json`. Then update the serde structs
  in `crates/core/src/schema/`, and regenerate the files made from the schema:
  `node scripts/gen-schema-types.mjs` (`packages/schema/index.d.ts`),
  `node scripts/gen-schema-validator.mjs` (`packages/react/src/tree/schema-validator.ts`),
  `python3 scripts/gen-schema-doc.py` (`docs/SCHEMA.md`) and
  `python3 scripts/gen-fuzz-dict.py` (`fuzz/render_json.dict`). Teach the fuzz generator
  (`fuzz/src/lib.rs`) the new properties, and add an entry to `packages/schema/CHANGELOG.md`.
  Changes must be backwards compatible within schema version 1.
- **Generated files** are committed. `scripts/check-generated.sh` runs every generator and fails
  when one of their files changed: besides the schema files above, the fixtures that
  `scripts/fixtures/` generates, the version `@inkoxide/react` reports
  (`scripts/gen-react-version.mjs`), the Tailwind theme (`scripts/gen-tailwind-theme.mjs`) and
  `docs/TAILWIND_SUPPORT.md` (`scripts/gen-tailwind-docs.ts`). After it fails, the regenerated
  files are in place: review and commit them.
- **React and Tailwind changes** need a test in `packages/*/test`. A new Tailwind utility
  belongs in `packages/tailwind/src/supported.ts`; its examples are tests, and
  `pnpm exec tsx scripts/gen-tailwind-docs.ts` regenerates `docs/TAILWIND_SUPPORT.md`.
- **Screenshots** in the READMEs come from `examples/`. After changing an example or the
  output, run `pnpm examples` (needs poppler) and commit the new images.
- **User-facing changes** get a line in [CHANGELOG.md](CHANGELOG.md) under "Unreleased".

## Pull requests

- Keep each pull request to one change, with a title that says what it does
  ("Repeat table footers on every page", not "Fix tables").
- The `ci` and `deny` workflows must pass, on Linux. `ci` runs formatting, clippy and the Rust
  tests; the TypeScript typecheck, lint, formatting and tests; the schema checks and the
  generated files; and, in its `wasm` job, the WebAssembly build, its size budget, its parity
  with the native build and the browser tests. `deny` runs `cargo-deny` on the dependencies.
- Rust is formatted with `cargo fmt` and linted with clippy (pedantic). TypeScript is
  formatted with oxfmt (`pnpm format`) and linted with oxlint (`pnpm lint`).

## Releasing

Maintainers release as [docs/RELEASING.md](docs/RELEASING.md) describes.

## Licence

By contributing, you agree that your contributions are licensed under the MIT licence of this
project.
