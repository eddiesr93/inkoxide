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
scripts/ci.sh --quick      # formatting, tests and schema checks
scripts/ci.sh              # everything, including clippy and the visual diff
```

- **Engine changes** need a test: a unit test next to the code, or a fixture in
  `fixtures/docs/` when the change affects output.
- **Output changes** show up in the visual regression test. Check `target/visual/*-diff.png`,
  and when the new output is right, regenerate the references with
  `scripts/update-expected.sh` (on Linux) and commit them with the change.
- **Schema changes** start in `packages/schema/document.v1.json`. Then update the serde structs
  in `crates/core/src/schema/`, run `node scripts/gen-schema-types.mjs`,
  `python3 scripts/gen-schema-doc.py` and `python3 scripts/gen-fuzz-dict.py`,
  teach the fuzz generator (`fuzz/src/lib.rs`) the new properties, and add
  an entry to `packages/schema/CHANGELOG.md`. Changes must be backwards compatible within schema version 1.
- **React and Tailwind changes** need a test in `packages/*/test`. A new Tailwind utility
  belongs in `packages/tailwind/src/supported.ts`; its examples are tests, and
  `pnpm exec tsx scripts/gen-tailwind-docs.ts` regenerates `docs/TAILWIND_SUPPORT.md`.
- **Screenshots** in the READMEs come from `examples/`. After changing an example or the
  output, run `pnpm examples` (needs poppler) and commit the new images.
- **User-facing changes** get a line in [CHANGELOG.md](CHANGELOG.md) under "Unreleased".

## Pull requests

- Keep each pull request to one change, with a title that says what it does
  ("Repeat table footers on every page", not "Fix tables").
- The `ci` workflow must pass. It runs formatting, clippy, the Rust and TypeScript tests and
  the schema checks on Linux.
- Rust is formatted with `cargo fmt` and linted with clippy (pedantic). TypeScript is
  formatted with oxfmt (`pnpm format`) and linted with oxlint (`pnpm lint`).

## Releasing

Maintainers release from `main`:

1. Bump the version of `inkoxide`, `@inkoxide/wasm`, `@inkoxide/render`, `@inkoxide/react`,
   `@inkoxide/tailwind` and `@inkoxide/style` together (the Cargo workspace and the six
   `package.json` files; `scripts/check-versions.py` checks them), since they share one version,
   and run `node scripts/gen-react-version.mjs`. Bump `@inkoxide/schema` only if the schema changed; it has its own
   version. Move the "Unreleased" entries in [CHANGELOG.md](CHANGELOG.md) and
   [packages/schema/CHANGELOG.md](packages/schema/CHANGELOG.md) under the new versions.
2. Dispatch the `release` workflow, first with `dry_run` on. It stops unless the `ci` workflow
   passed for the commit (a dry run only warns). It builds the native bindings on every platform
   and the WebAssembly engine, and publishes every package whose version is not on npm yet, in
   dependency order: `@inkoxide/wasm` with the engine packages, before `@inkoxide/render`. npm trusts
   the workflow (trusted publishing), so no token is involved and every package is published
   with provenance. A real release publishes from the `release` environment, which admits only
   `main`; npm trusts that environment alone. Then it tags the commit and creates the GitHub
   release from the newest changelog section: `v0.2.0` for a heading `## 0.2.0 - <date>`,
   `@inkoxide/render@0.1.1` for `## @inkoxide/render 0.1.1 - <date>`.

A new package needs its trusted publisher on npm (the `release.yml` workflow and the `release`
environment) before the workflow can publish it. Where npm lets that be set only on a package
that exists, the maintainer publishes its first version by hand, from the staged copy
(`node scripts/npm-release.mjs <bin-dir> <staging-dir>`), then sets the trusted publisher.

The workflow also publishes the crates `inkoxide` and `inkoxide-cli` to crates.io, after the npm
packages and with the versions not on crates.io yet. crates.io trusts the workflow too: each crate
has a trusted publisher there for `release.yml` and the `release` environment. A new crate is
published by hand the first time (`cargo publish -p <crate>`), since crates.io sets a trusted
publisher only on a crate that exists. The other crates are not published.

## Licence

By contributing, you agree that your contributions are licensed under the MIT licence of this
project.
