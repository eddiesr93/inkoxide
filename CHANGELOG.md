# Changelog

All notable changes to the packages in this repository. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and every package follows
[Semantic Versioning](https://semver.org/). The document schema has its own
[changelog](packages/schema/CHANGELOG.md).

## Unreleased

### @inkoxide/render 0.1.1

- A new README with examples, and two new guides: [GUIDE.md](packages/render/GUIDE.md) and
  [MIGRATING.md](packages/render/MIGRATING.md).
- `INKOXIDE_ASSETS_DIR` replaces `STUDIO_ASSETS_DIR` for loading the bundled fonts from another
  directory.
- `StudioError` and `StudioErrorCode` are deprecated aliases of `RenderError` and
  `RenderErrorCode`.

### Repository

- `@inkoxide/tailwind`, `@inkoxide/react` and `@inkoxide/render` are developed in this
  repository, under `packages/`, with the examples and the benchmarks.
- Continuous integration on every pull request.

## 0.1.0 - 2026-10-11

First release.

- `inkoxide` 0.1.0: the engine as a Node package, with prebuilt binaries for macOS (arm64, x64),
  Linux (x64 glibc and musl, arm64 glibc) and Windows (x64).
- `@inkoxide/schema` 1.0.0: TypeScript types and the JSON Schema of document schema version 1.
- `@inkoxide/tailwind` 0.1.0, `@inkoxide/react` 0.1.0 and `@inkoxide/render` 0.1.0: React
  components to PDF, styled with Tailwind classes or style objects.
