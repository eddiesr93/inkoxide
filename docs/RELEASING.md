# Releasing

Maintainers release from `main`, through the `release` workflow (`.github/workflows/release.yml`).

## What is released

| What | Version |
|---|---|
| `inkoxide`, its platform packages (`@inkoxide/<platform>`), `@inkoxide/wasm`, `@inkoxide/render`, `@inkoxide/tree`, `@inkoxide/react`, `@inkoxide/vue`, `@inkoxide/svelte`, `@inkoxide/angular`, `@inkoxide/tailwind` and `@inkoxide/style` | One shared version |
| The crates `inkoxide` and `inkoxide-cli` | The same version, from the Cargo workspace |
| `@inkoxide/schema` | Its own version, bumped only when the schema changes |

The other crates (`inkoxide-node`, `inkoxide-wasm`, `inkoxide-binding`) are not published.
`scripts/check-versions.py` checks that the shared versions agree; CI runs it.

## Steps

1. **Bump the versions** of the packages and crates above together: the Cargo workspace and the
   ten `package.json` files. Run `node scripts/gen-react-version.mjs`, which writes the version
   `@inkoxide/react` reports. Bump `@inkoxide/schema` only if the schema changed.
2. **Move the "Unreleased" entries** in [CHANGELOG.md](../CHANGELOG.md) and
   [packages/schema/CHANGELOG.md](../packages/schema/CHANGELOG.md) under the new versions.
3. **Merge to `main`** and wait for the `ci` workflow to pass on that commit.
4. **Dispatch the `release` workflow with `dry_run` on** (the default). It builds everything,
   runs `npm publish --dry-run` and `cargo publish --dry-run`, and prints the tag and the
   release notes. Nothing is uploaded.
5. **Dispatch it again with `dry_run` off.**

A run that fails part way can be dispatched again: every step skips what is already published.

## What the workflow does

1. **`ci-passed`** stops the release unless the `ci` workflow passed for the commit. A dry run
   only warns.
2. **Builds:** the native bindings on Linux (x64 glibc and musl, arm64), macOS (arm64, x64) and
   Windows (x64), and the two WebAssembly modules, `inkoxide.wasm` and `inkoxide-threads.wasm`,
   which must stay under their size budget (`scripts/wasm-size.mjs`). The module with threads is
   built on the nightly that `scripts/build-wasm.sh --toolchain` prints, with `rust-src`. The platforms come from `napi.targets` in
   `packages/inkoxide/package.json` (`scripts/platforms.mjs`).
3. **`publish`**, to npm, in dependency order, skipping every `name@version` already on the
   registry:
   - `scripts/npm-release.mjs` publishes the engine packages: the platform packages, then
     `inkoxide`, `@inkoxide/schema` and `@inkoxide/wasm`. The README of `@inkoxide/wasm` gets
     the version and the Subresource Integrity hash of each of its two modules.
   - `scripts/release/publish.sh` then publishes `@inkoxide/style`, `@inkoxide/tailwind`,
     `@inkoxide/tree`, `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/vue`, `@inkoxide/svelte`
     and `@inkoxide/angular`.
4. **`crates`** publishes `inkoxide` and then `inkoxide-cli` to crates.io, skipping a version
   already there.
5. **`github-release`** tags the commit and creates the GitHub release from the newest
   changelog section: `v0.2.0` for a heading `## 0.2.0 - <date>`, `@inkoxide/render@0.1.1` for
   `## @inkoxide/render 0.1.1 - <date>`.

The `test-windows` workflow runs the Node binding tests on Windows against the binary a release
run built; dispatch it with that run's ID.

## Trusted publishing

npm and crates.io both trust the workflow, so no publishing token is stored:

- **npm:** each package has a trusted publisher for `release.yml` and the `release` environment.
  Every package is published with provenance.
- **crates.io:** each crate has a trusted publisher for the same workflow and environment. The
  job gets a short-lived token through OIDC, revoked when the job ends.

A real release publishes from the `release` environment, which admits only `main`. A dry run
uses no environment and may come from any branch.

A **new package or crate** needs its trusted publisher before the workflow can publish it. Where
the registry lets that be set only on a package that exists, the maintainer publishes the first
version by hand, then sets the trusted publisher:

- an engine package: from the staged copy, `node scripts/npm-release.mjs <bin-dir> <staging-dir>`;
- a TypeScript package (`@inkoxide/tree`, a front end): `pnpm release:publish`, logged in with `npm login`;
- a crate: `cargo publish -p <crate>`.

## The release scripts

| Command | What it does |
|---|---|
| `pnpm release:stage` | `scripts/release/stage.mjs`: builds `@inkoxide/style`, `@inkoxide/tailwind`, `@inkoxide/tree`, `@inkoxide/render`, `@inkoxide/react`, `@inkoxide/vue`, `@inkoxide/svelte` and `@inkoxide/angular` with `tsc` (the Svelte components are copied as they are), and writes publish-ready copies to `tmp/release/<package>/`, with `exports` pointing at `dist/`, registry versions in place of `workspace:` links, `engines` from the root `package.json`, each package's `README.md`, and the fonts and hyphenation patterns of `@inkoxide/render`. The workspace stays as it is. `--list` prints the packages in publish order |
| `pnpm release:publish` | `scripts/release/publish.sh`: stages, checks that the engine packages they depend on (`inkoxide`, `@inkoxide/schema`) are on the registry, waiting a few minutes for a version just published, and publishes each package not on the registry yet. `--dry-run` stages and runs `npm publish --dry-run`. In GitHub Actions it uses trusted publishing; locally, the `npm login` session, with 2FA in the browser |
| `node scripts/npm-release.mjs <bin-dir> <staging-dir> [--publish [--dry-run]]` | Stages the engine packages from a directory holding a native binary for every platform, `inkoxide.wasm` and `inkoxide-threads.wasm`, and with `--publish` publishes them |
| `node scripts/release/github-release.mjs [--dry-run] [--section <name>] [--target <commit>]` | Creates the tag and the GitHub release for a changelog section (by default the newest released one), with `gh`. An existing release is left alone |
