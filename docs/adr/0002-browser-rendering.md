# 2. Rendering in the browser with the engine as WebAssembly

Status: accepted, 2026-10-15; shipped in 0.9.0. The decision **One thread**, and the rejection of
a nightly toolchain for the module with threads, are superseded by
[ADR 0003](0003-browser-threads.md).

## Context

Templates written for `@inkoxide/render` should render in a browser too, to the same PDF bytes
as in Node, without special server setup: no cross-origin isolation, and no `unsafe-eval` in the
Content Security Policy. The engine compiles for `wasm32-unknown-unknown` unchanged, but:

- the standard library for that target is built with `panic = "abort"`: `catch_unwind` catches
  nothing, and a panic stops the instance. Two limits stopped work by unwinding: the shaping glyph
  limit and the colour glyph paint budget;
- threads need shared memory, a nightly toolchain and cross-origin isolation;
- the module was about 2 MB with brotli, more than half of it document parsing (four ways:
  JSON and MessagePack, each with and without path tracking) and hypher's built-in patterns;
- on WebAssembly, Rust's trigonometry is its own port of libm, not the platform's, so numbers a
  dependency computes with it (usvg's marker angles) can differ in the last digits;
- WebAssembly memory only grows.

## Decision

The engine ships as `@inkoxide/wasm`, and `@inkoxide/render/browser` renders with it in a Web
Worker.

- **Panics: stable toolchain, `panic = "abort"`.** The two unwinding limits return errors
  instead. The first shaping failure is kept in the render's shape caches, since shaping runs
  inside the layout engine's measure callback, which has no error path, and is returned after
  layout as `FONT_ERROR`. A COLRv1 glyph's paint graph is counted before it is painted. A panic
  in a dependency still traps: the JS wrapper drops the instance, starts a new one from the
  compiled module, registers its fonts and patterns again, and fails the call with `INTERNAL`.
- **Binding: a hand-written C ABI and a small JS wrapper, not wasm-bindgen.** Byte buffers in,
  one result buffer out; options as JSON. The module has no imports (the module with threads of
ADR 0003 imports its memory), and there is no generated
  glue to keep in step with a tool version. Both bindings share their option checks through
  `crates/binding`. Unsafe code stays in `crates/wasm`.
- **Entry point: a subpath, `@inkoxide/render/browser`**, not an export condition, so a server
  bundle never picks it by accident. `@inkoxide/wasm` is an optional peer of `@inkoxide/render`,
  so Node installs do not download it.
- **Size.** `opt-level = 3`, for speed, and no wasm-opt: binaryen at `-O3`, `-Os` and `-Oz` made
  the module no smaller with brotli and no faster. The WebAssembly build turns off the cargo
  features `msgpack` (it takes JSON only) and `embedded-hyphenation`. The engine matches words
  against hypher's patterns itself (`text/builtin.rs`), and `@inkoxide/wasm` fetches a
  language's patterns when a document first hyphenates text in it. CI fails over a budget of
  1.45 MB with brotli (`scripts/wasm-size.mjs`).
- **Fonts on demand.** `RenderOptions::deferred_fonts` names declared fonts that may be missing:
  a render that does not draw with one, nor try it for a character, gives the bytes it would give
  with the font; one that does fails with `NOT_LOADED`, naming it. The browser entry defers Noto
  Sans, the fallback font, and fetches it only when a character needs it. The bundled fonts are
  fetched as TTF, not WOFF2, whose decoding rebuilds `glyf` and `loca` and so changes the bytes.
- **Parity.** `scripts/wasm-parity.mjs` renders every fixture with both builds and compares the
  bytes. Fixtures listed in `fixtures/wasm-parity.json` may differ only in numbers, within 1e-6,
  and in `/ID`. Making them exact would need patched dependencies.
- **One thread** (superseded by ADR 0003, which adds a module with threads). On WebAssembly a
  pool of more than one thread is refused early; `threads` takes only 1. The process-wide caches get a quarter of their native budgets, and
  `inkoxide::clear_caches()` empties them.
- **The worker.** One worker runs the renders in order and keeps its fonts registered. Images
  are copied to it, since a render may need them again; the PDF is transferred back. `signal`
  and `timeoutMs` end the worker and start a new one; `timeoutMs` rejects with a `DOMException`
  named `TimeoutError`. In Node, where the native render cannot be stopped, neither has an
  effect on it.
- **No Node in shared code.** `@inkoxide/react` validates trees in development with a validator
  Ajv generates ahead of time (`scripts/gen-schema-validator.mjs`), so no page needs
  `unsafe-eval`, and reads its version from a generated file (`scripts/gen-react-version.mjs`),
  not with `createRequire`; both files are committed and checked in CI. The engine and the
  bundled assets come through an `EngineHost`, with `node:fs` and `inkoxide` in Node, and `fetch`
  and `@inkoxide/wasm` in a worker in the browser.
- **`Font.register` in the browser** takes a URL, relative to the page as in a link, or bytes. A
  file path that cannot be a URL fails the render with `INVALID_FONT`.
- **Release.** `@inkoxide/wasm` is released with the other packages, at their version. Its npm
  README gets the module's Subresource Integrity hash at staging.
- **Runtimes.** Browsers and Node only; edge runtimes are out of scope.

## Consequences

- The same template renders in Node and in a browser to the same bytes, but for the last digits
  of numbers that dependencies compute with trigonometry, in listed fixtures.
- Native behaviour is unchanged: native output stayed byte-identical through the engine
  changes.
- A panic in a dependency costs the browser a new instance (about 10 ms) and the call fails;
  natively it is an error and the process goes on.
- Renders in the browser use one thread, and an instance has at most 4 GiB of memory (ADR 0003:
  the module with threads renders on up to 256, with at most 2 GiB).
- Not available in the browser: `renderToFile`, `renderToStream`, and local image files
  (`localImages`).

## Alternatives

- A nightly toolchain with `-Zbuild-std` and WebAssembly exception handling: native behaviour
  without changing the limits, but a nightly toolchain and an unstable target feature in every
  release, and a larger module.
- wasm-bindgen: generated glue that must match the `wasm-bindgen-cli` version exactly.
- A `browser` export condition on `@inkoxide/render`: a bundler could pick it for server code.
- `opt-level = "s"`: about 11% smaller, about 16% slower.
- WOFF2 for the bundled fonts: a smaller download, but other bytes than Node.
