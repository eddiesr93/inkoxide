# Plan: rendering in the browser

Written on 2026-10-15 against `main` at `863a355` (0.8.0); line references point to that
commit. Implemented on the branch `feat/client-render`, with the recommendation of each open
decision. Where the implementation differs from this plan:

- **Glyph limit (E1).** The first shaping failure is kept in the shape caches and returned after
  layout, since shaping runs inside the layout engine's measure callback.
- **The binding** shares its option checks through a crate, `crates/binding`.
- **No wasm-opt.** binaryen 133 at `-O3`, `-Os` and `-Oz` made the module no smaller with brotli
  (1.97 MB then) and no faster, so the release ships the module CI tests.
- **Built-in hyphenation patterns (E4)** do not stay in the module. The engine matches words
  against hypher's patterns itself (`text/builtin.rs`, tested against hypher in every language),
  and a cargo feature, `embedded-hyphenation`, on by default, embeds them. The WebAssembly build
  leaves them out; `@inkoxide/wasm` fetches a language's file when a document first hyphenates
  text in it, and hyphenates as Node does. The module is 1.18 MB with brotli.
- **Deferred fonts.** `RenderOptions::deferred_fonts` names declared fonts that may be missing.
  A render that does not draw with one, nor try it for a character, gives the bytes it would
  give with the font; one that does fails with `NOT_LOADED`, naming it. The browser entry
  defers the bundled fallback font, Noto Sans, which `@inkoxide/wasm` fetches only when a
  character needs it.
- **The size budget** is 1.3 MB with brotli, from the build with both changes above.
- **Images** are copied to the worker, since a render may need them again; the PDF is
  transferred.
- **`@inkoxide/wasm`** is an optional peer of `@inkoxide/render`, so that Node installs do not
  download it.
- **`examples/browser`** has an invoice of its own: the Node examples import Node modules.
- **Edge runtimes (Decision 7)** are out of scope: the supported runtimes are browsers and
  Node, and the docs name only those.
- **The browser benchmark** (`benchmarks/browser`) measures Chromium and WebKit; the Playwright
  build of Firefox does not start on the machine it was run on.
- **Browser tests.** `packages/render/test/browser.test.tsx` runs the browser entry in Node, on
  the calling thread (`worker: false`). The Playwright tests on `examples/browser` render in a
  worker in Chromium, Firefox and WebKit, through the bundler: the invoice, and a letter for
  which the engine fetches German hyphenation patterns and Noto Sans.
- **`timeoutMs`** is an option of the render, as `signal` is: it stops the render the same way,
  which rejects with a `DOMException` named `TimeoutError`. In Node the native render cannot be
  stopped, and neither has an effect on it.
- **`Font.register` in the browser:** text is a URL, relative to the page, as in a link. A file
  path that cannot be one (a `file:` URL, a drive letter or a backslash) fails the render with
  `INVALID_FONT`.
- **The React version** is generated from `package.json` by `scripts/gen-react-version.mjs`,
  which staging runs before the build; the generated file is committed, and CI checks it.
- **SRI.** The README `@inkoxide/wasm` publishes gets the hash of its module at staging.

It covers the [ROADMAP.md](../../ROADMAP.md) "Later" item **Rendering in the browser, through a
WebAssembly build of the engine** (ROADMAP.md:8).

## Summary

- **The engine compiles for `wasm32-unknown-unknown` unchanged.** A throwaway spike, kept
  outside the repository, rendered all 39 fixtures through a WebAssembly build in Node 22. Its
  numbers are below.
- **It ships in three pieces:**
  1. `crates/wasm`, a small binding with the contract of the Node binding;
  2. a new npm package, `@inkoxide/wasm`;
  3. a browser entry, `@inkoxide/render/browser`, which renders in a Web Worker.
- **The engine needs two changes before release:**
  - the two limits that stop work by unwinding, because in WebAssembly a panic stops the
    instance;
  - a parity test against the native build.

  The other engine changes are about download size and memory.
- **Effort:** about 22–31 working days for phases 1–4.

## What the spike measured

**Setup**

- **Build:** Rust 1.97.0 for `wasm32-unknown-unknown`, the workspace release profile (fat LTO,
  one codegen unit), `panic = "abort"`, symbols stripped, and no wasm-opt.
- **Run:** Node 22.23 on arm64 Linux, one thread, against the same engine built natively for
  arm64 Linux.
- **Timing:** each fixture was rendered 7 times. "Warm" is the median of the 7 and "cold" is the
  first.

| Build | Raw | gzip -9 | brotli -q 11 | Time vs native, warm / cold |
|---|---|---|---|---|
| `opt-level = 3` | 8.43 MB | 2.84 MB | 2.16 MB | 1.74× / 2.11× |
| `opt-level = "s"` | 7.69 MB | 2.46 MB | 1.93 MB | 2.06× / 2.34× |
| `opt-level = "z"` | 6.97 MB | 2.35 MB | 1.86 MB | 4.36× / 4.34× |
| `opt-level = 3`, unwinding (nightly, see Panics) | 9.08 MB | 2.99 MB | 2.28 MB | 1.79× / 2.24× |

**Speed and memory**

- The 98-page `report-100p-realistic` renders in 150 ms; natively it takes 92 ms.
- `invoice` renders in 2.0 ms; natively it takes 0.9 ms.
- Instantiating the module from bytes takes about 11 ms.
- Memory after all 39 fixtures is 27 MiB.
- These numbers are from V8. Firefox and Safari were not measured.
- `+simd128` made no difference to the size; it was not timed.

**Where the size goes**

- Code is 6.6 MB. About 43% of it deserialises the document:
  - `serde_path_to_error` 1.09 MB;
  - `rmp_serde` 1.00 MB;
  - `serde_core` 0.69 MB;
  - `serde_json` 0.10 MB.
- `Document` is deserialised four ways: JSON and MessagePack, each with and without path
  tracking (`validate/parse.rs:41-110`).
- The engine itself is 1.17 MB, rustybuzz 0.24 MB, taffy 0.22 MB and usvg 0.21 MB.
- Data is 1.76 MB. About 1.2 MB of that is hypher's built-in patterns (its `full` feature, the
  default).

**Output bytes**

- 38 of 39 fixtures are byte-identical to the native build.
- `svg-markers` differs in one marker's rotation matrix, in the last digits (`-0.41321623`
  against `-0.41321635`), and therefore also in `/ID`.
- Native macOS and native Linux agree on all 39.
- The cause is the math library:
  - natively, Rust's `f32`/`f64` trigonometry calls the platform's libm;
  - on WebAssembly it uses Rust's own port of libm;
  - usvg computes the marker angle, so the difference comes from a dependency.

**Panics**

- The standard library for `wasm32-unknown-unknown` is built with `panic = "abort"`. So
  `catch_unwind` catches nothing, and a panic stops the instance with
  `RuntimeError: unreachable`.
- Two limits unwind on purpose. In WebAssembly both stop the instance instead of returning:
  - **the shaping glyph limit** (`text/prepare.rs:594`): native gives `FONT_ERROR`;
  - **the colour glyph paint budget** (`fonts/color.rs:122`, `218-221`): natively the glyph
    is drawn without colour.
- Both behave as native when built with `nightly-2026-10-11 -Zbuild-std=std,panic_unwind
  -Cpanic=unwind -Ctarget-feature=+exception-handling`, which uses WebAssembly exception
  handling.
- On stable, `-Cpanic=unwind` changes nothing for this target.

**Threads**

- rayon's global pool falls back to the calling thread where threads are unsupported
  (rayon-core 1.13, `registry.rs:209-227`). So `threads: None` works.
- A pool of more than one thread (`pool.rs:29`) fails to build.

**Fonts**

- A font registered as WOFF gives the same PDF bytes as its TTF.
- As WOFF2 it does not, because WOFF2 decoding rebuilds `glyf` and `loca`.
- Over HTTP with brotli, Inter Regular is 158 KB and Inter Bold 161 KB.

## Goals

1. Render a document in the browser, in a Web Worker, from the same React templates as on the
   server, with the same output.
2. Same engine, same limits and same error codes as native.
3. No special server setup:
   - no cross-origin isolation (COOP/COEP);
   - no `unsafe-eval` in the Content Security Policy, only `wasm-unsafe-eval`.
4. At most 2 MB (brotli) to download for the engine. Fonts are loaded per document.
5. Usable without React (from JSON), through `@inkoxide/wasm`.

## Not in the first release

- Threads inside a render. They need shared memory, a nightly toolchain and COOP/COEP.
- Reading local files, `renderToFile`, and `renderToStream` as a Node stream.
- A PDF viewer of our own. The browser's own viewer shows the blob.
- Byte-identical numbers where dependencies use trigonometry (Decision 5).

## Design

```
main thread                                   worker
React element ── renderToTree ──► document JSON ──────────────┐
fonts (fetched once, kept) ───────────────────► registerFont   │
images (fetched or bytes) ─────────────────────► render(doc, { images }) ──► PDF bytes ──► Blob
```

### 1. `crates/wasm`: the binding

- **Shape.** A `cdylib` for `wasm32-unknown-unknown`, with `publish = false`, like
  `crates/node`.
- **Contract.** It exposes the contract of `packages/inkoxide/index.d.ts`: `registerFont`,
  `unregisterFont`, `registerHyphenationPatterns`, `unregisterHyphenationPatterns`, `validate`,
  `render`, `renderSync` and `version`. So `@inkoxide/render` can use either binding behind
  `engine.ts`.
- **A hand-written C ABI with a small JS wrapper, not wasm-bindgen** (Decision 2):
  - Each call passes byte buffers, which the wrapper allocates through `alloc` and the binding
    frees. Each call gets one result buffer back: the PDF bytes, or `{code, path, message}` as
    JSON for an error.
  - Options go as JSON. The binding reuses the Node binding's option checks
    (`crates/node/src/lib.rs:113-293`), moved into a module both bindings share, with the same
    tests.
  - The images of a render go as one buffer: their count, then for each image the id length,
    id, data length and data.
  - There is no generated glue to keep in step with a `wasm-bindgen-cli` version, which must
    match the crate exactly. The module has no imports.
- **State.** One `FontStore` and one `HyphenationStore` per instance, as the Node binding keeps
  one per process (`crates/node/src/lib.rs:31-34`).
- **`threads`.** Only `1`, or left out. Anything else is `INVALID_ARGUMENT` at
  `options.threads`.
- **Unsafe code** stays in this crate. `crates/core` keeps `#![forbid(unsafe_code)]`.

### 2. Engine changes (`crates/core`)

Each change lands on its own, and native output stays byte-identical.

**E1. Limits without unwinding** (Decision 1, the recommended option)

- **Glyph limit.**
  - Shaping runs inside the layout engine's measure callback, which has no error path. So the
    first failure (the glyph limit, or a shaper panic caught natively) is kept in the render's
    shape caches, which every shaping thread shares.
  - After it, no more text is shaped, and `render_inner` returns it as `FONT_ERROR` once SVG
    text and layout are done.
  - `panic_error` (`lib.rs:316`) then handles only real panics.
  - The `FONT_ERROR` message stays the same.
- **Colour budget.**
  - Before painting a COLRv1 glyph, count its paint graph (landed in #119):
    - every node reached counts, with multiplicity, and the count stops at `MAX_PAINT_OPS`;
    - counts are memoised per paint offset;
    - a cycle ends the glyph, as it does in ttf-parser.
  - Over the limit, the glyph has no colour, as now.
  - Painting is then bounded whatever the painter is called with. So `OverBudget` and its
    `catch_unwind` go (`fonts/color.rs:110-132`, `218-221`). The painter keeps its call and
    markup budget, but past it its calls do nothing, and the glyph has no colour.
  - Native output stays the same for every fixture and every test in `tests/limits.rs`.
- **The other `catch_unwind` calls stay:** the one around shaping (`text/prepare.rs:599`) and
  the one around the render (`lib.rs:301`). Natively they still turn a panic in rustybuzz or
  another dependency into an error.
- **In WebAssembly, a dependency panic** stops the instance. The JS wrapper then starts a fresh
  instance (section 3), and the render fails with `INTERNAL`.

**E2. Threads**

- On WebAssembly (`cfg(target_family = "wasm")`), `pool::get` (`pool.rs:20`) refuses a pool of
  more than one thread with a message that says threads are not available, instead of
  "thread pool: …".

**E3. Cache budgets**

- The process-wide caches hold up to 128 MiB:
  - images, `images/cache.rs:17`: 64 MiB;
  - font subsets, `pdf/font.rs:70`: 32 MiB;
  - SVG text, `images/svg.rs:424`: 32 MiB.
- WebAssembly memory only grows. So on WebAssembly the caches get a quarter of those budgets.
- A new `inkoxide::clear_caches()` lets the binding empty them between renders. The memory goes
  back to the instance for reuse, not to the browser.

**E4. Size**

- **MessagePack.** A cargo feature `msgpack`, on by default, controls `InputFormat::MessagePack`
  (`validate/parse.rs:16`, `24-80`).
  - The WebAssembly build turns it off and takes JSON only; the JS side sends
    `JSON.stringify` anyway.
  - Estimated saving: about a fifth of the code. The PR measures it.
- **Built-in hyphenation patterns** come at run time in the WebAssembly build, so that text
  breaks as in Node (see the list of changes at the top).

**E5. Parity**

- A new `scripts/wasm-parity.mjs` renders every fixture with both builds and compares the
  bytes.
- Fixtures listed in `fixtures/wasm-parity.json` may differ only in two ways, checked after
  decompressing the streams:
  - in numbers, within 1e-6 (relative);
  - in `/ID`.
- Today the list holds `svg-markers`.

### 3. `@inkoxide/wasm`: the package

**Files**
- `inkoxide.wasm`;
- `index.js` (an ES module);
- `index.d.ts`: the contract of the `inkoxide` package, plus `init`;
- `README.md`.

The package has `sideEffects: false`.

**`init(source?)`**
- Compiles the module once (with `WebAssembly.compileStreaming` where available) and
  instantiates it.
- `source` is one of: a URL, a `Response`, bytes, or a compiled `WebAssembly.Module`.
- The default source is `new URL('./inkoxide.wasm', import.meta.url)`. Vite, webpack 5,
  Rollup and esbuild with a loader resolve it.
- The other functions throw until `init` resolves.

**`render`**
- Returns a promise but runs on the calling thread. In a page, call it from a worker;
  `@inkoxide/render/browser` does.

**After a trap** (a `WebAssembly.RuntimeError`), the wrapper:
1. drops the instance;
2. instantiates a new one from the compiled module, in about 10 ms;
3. registers again the fonts and patterns it keeps;
4. rejects the call with `InkoxideError` `INTERNAL`, "the engine stopped: …".

Running out of memory, at the 4 GiB WebAssembly limit or the browser's own, ends here too.

**In Node**
- The binding contract tests (`packages/inkoxide/test/*.test.js`) run against both packages.
- It can stand in on platforms without a native binary. `inkoxide` does not fall back to it on
  its own.

### 4. `@inkoxide/render/browser`

**API** (a subpath export, Decision 3)

- `init(opts?)`:
  - `wasm`: as `init` above;
  - `assets`: where the bundled fonts and patterns are; by default, next to the module;
  - `worker`: default `true`.
- `renderToBuffer(element, opts)` gives a `Uint8Array`.
- `renderToBlob(element, opts)` gives a `Blob` (`application/pdf`).
- `generate` (templates), `Font.register` (a URL or bytes) and `version`.
- Not available: `renderToFile`, `renderToStream` and `localImages`.

**React helpers** (phase 4)

- `usePdf(element, opts)` gives `{ blob, url, loading, error, update }`.
- `<PdfDownloadLink>` and `<PdfViewer>` (an `<iframe>` of the blob URL) are thin components.
- They cover the browser APIs that MIGRATING.md:39 lists as not available.

**Changes to the code Node shares**

- **Engine host** (`engine.ts:5-30`, `70-115`): the engine and the bundled assets come through
  an interface, `EngineHost`:
  - a Node implementation, with `node:fs` and `inkoxide`;
  - a browser implementation, with `fetch` and `@inkoxide/wasm` in a worker.
- **Fonts:**
  - The default faces (`fonts.ts:12-19`) are fetched when a document uses them, not at start-up.
  - They are fetched as TTF, for byte parity; WOFF2 gives other bytes.
  - `Font.register` takes a URL or bytes. A file path is an error in the browser.
- **Node globals:**
  - `Buffer` (`engine.ts:228`, `images.ts:289`, `font.ts:90-94`, `local.ts:98`) becomes
    `Uint8Array`, with `TextDecoder('latin1')` and `atob`.
  - `process.env.NODE_ENV` (`pipeline.ts:34`, `tailwind/src/index.ts:86`) is read through a
    guard.
- **Remote images:**
  - The same allowlist, timeout and size limit as in Node (`remote.ts:53-100`).
  - Requests use `credentials: 'omit'` and `mode: 'cors'`.
  - A redirect is refused through `redirect: 'manual'`, which the browser reports as an
    `opaqueredirect` response.
- **`@inkoxide/react`:**
  - The package version is generated at build time, instead of read with `createRequire`
    (`reconciler/host-config.ts:28`).
  - Schema validation in development uses a validator Ajv generates at build time
    (standalone code), not `ajv.compile` (`tree/validate.ts:18-22`), which needs
    `unsafe-eval`.
  - `fonts.loadDirectory` (deprecated, `fonts/registry.ts:84`) stays Node-only.
- **`@inkoxide/tailwind`:**
  - A browser entry loads the design system from `tailwindcss` itself, whose
    `__unstable__loadDesignSystem` takes a `loadStylesheet` callback.
  - It bundles `tailwindcss/theme.css` as text, instead of reading it through
    `@tailwindcss/node` (`compile.ts:1-4`, `28`, `52`).
  - It is loaded only when a template uses `className`, as now (`react/src/tailwind.ts`).

**The worker**

- Created with `new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })`.
- One worker runs the renders in order.
- Fonts are registered in it once.
- Documents, images and PDFs are transferred to and from it, not copied.
- `signal` and `timeoutMs` end the worker (`terminate()`) and start a new one. A native render
  cannot be stopped that way.

## Limits and security

- **Limits.**
  - The engine and its limits are the same, so SECURITY.md "Untrusted input" applies.
  - An instance has at most 4 GiB of memory (a WebAssembly limit).
  - Rendering in a worker keeps the page responsive, and lets a runaway render be ended.
- **Content Security Policy.**
  - Pages need `script-src 'wasm-unsafe-eval'`, plus `worker-src` for the worker.
  - Once Ajv is precompiled, nothing needs `unsafe-eval`.
- **Remote images.**
  - They stay off unless allowed, as on the server. In a browser, the requests come from the
    user's network position, so the allowlist still matters.
  - Credentials are never sent.
- **Build provenance.**
  - The release workflow builds the WebAssembly module and publishes it with provenance, like
    the native binaries.
  - The README gives an SRI hash for use from a CDN.
- **Fuzzing.**
  - The fuzz targets stay native.
  - A CI job renders the fuzz seed corpus and the `tests/limits.rs` documents through the
    WebAssembly build and compares the error codes with native. That catches a path that
    behaves differently, as the two unwinding limits do today.

## Tests

- **`crates/wasm`:**
  - Rust unit tests for the buffer format;
  - `cargo build --target wasm32-unknown-unknown -p inkoxide-wasm` in CI.
- **Size budget.** CI fails when the brotli size goes over a budget set from the build (1.3 MB),
  and prints the size in the job summary.
- **Parity (E5)** runs in CI on every pull request that touches `crates/`.
- **Contract.** The tests in `packages/inkoxide/test` run against `@inkoxide/wasm` in Node.
- **Browser.** Playwright tests (Chromium, Firefox, WebKit) on `examples/browser`, a Vite app:
  - render the invoice in the worker and compare its SHA-256 with the Node render;
  - render on a page with a strict CSP;
  - render on a page without COOP/COEP;
  - stop a render with `signal`.
- **`@inkoxide/render/browser`:** unit tests with Vitest browser mode, or Playwright component
  tests.

## Release

- `@inkoxide/wasm` joins the packages released together (`scripts/check-versions.py`,
  CONTRIBUTING.md "Releasing"). It is published before `@inkoxide/render`.
- **`release.yml` gets a `wasm` job on Ubuntu:**
  - the pinned toolchain and the `wasm32-unknown-unknown` target;
  - binaryen (`wasm-opt`), downloaded from its release asset with a checked SHA-256, the way
    `deny.yml` installs cargo-deny.
  - Its artifact goes to the `publish` job.
- **npm:**
  - The new package needs a trusted publisher bound to `release.yml` and the `release`
    environment.
  - Check whether npm accepts that before the first version. If not, the maintainer publishes
    the first version by hand.
- **`stage.mjs`:**
  - `@inkoxide/render` gains `exports["./browser"]` and the worker file;
  - `@inkoxide/tailwind` gains a `browser` export condition.

## Docs

- **README** "Platforms" (`README.md:180-189`): add browsers (current versions).
- **GUIDE.md:**
  - The FAQ "Does it run in the browser?" (`GUIDE.md:1209-1211`) and the "Server only"
    limitation (`GUIDE.md:1254-1255`) change.
  - A new section, "In the browser", covers: installing, `init`, the worker, fonts, images and
    CORS, the Content Security Policy, what is not available, and download sizes.
- **MIGRATING.md:39:** the browser line maps to `renderToBlob`, `usePdf`, `<PdfDownloadLink>`
  and `<PdfViewer>`.
- **docs/API.md and docs/CLIENT_INTEGRATION.md:** `@inkoxide/wasm`, and how it differs from the
  native build:
  - one thread;
  - `INTERNAL` after a panic in a dependency;
  - the parity note.
- **SECURITY.md:** the browser notes above.
- **docs/ARCHITECTURE.md:** `crates/wasm`.
- **docs/TESTING.md:** the parity and browser tests.
- **ROADMAP.md:** the item leaves "Later".
- **benchmarks/:** a browser benchmark against the library MIGRATING.md covers, in the same
  browser, reported in docs/BENCHMARKS.md.

## Commits

Each commit is green. Native output stays byte-identical until the docs commits.

1. `core`: the glyph limit returns an error instead of unwinding (E1).
2. `core`: the colour glyph budget stops painting without unwinding (E1). The paint graph
   count it relies on landed in #119.
3. `core`: a `msgpack` feature, on by default (E4).
4. `core`: on WebAssembly, smaller cache budgets, `clear_caches`, and threads refused early
   (E2, E3).
5. `crates/wasm` with its JS wrapper, and the `@inkoxide/wasm` package (private until the
   release).
6. CI: the WebAssembly build, the size budget, and the parity and limits-parity jobs (E5).
7. `@inkoxide/render`: the `EngineHost` interface with the Node implementation. Behaviour does
   not change.
8. `Buffer` and `process.env` out of shared code; Ajv standalone; a generated package version.
9. The `@inkoxide/tailwind` browser entry.
10. `@inkoxide/render/browser`: the worker, fonts, images, `renderToBuffer` and
    `renderToBlob`.
11. `examples/browser` and the Playwright tests.
12. React helpers: `usePdf`, `<PdfDownloadLink>` and `<PdfViewer>`.
13. The release workflow and staging, the docs, and ROADMAP.md.

## Effort (working days)

| Phase | Commits | Days |
|---|---|---|
| 1. Engine | 1–4 | 4–6 |
| 2. Binding and package | 5–6 | 5–7 |
| 3. Browser entry | 7–11 | 9–12 |
| 4. React helpers, release, docs | 12–13 | 4–6 |
| **Total** | | **22–31** |

Later, as separate work:

- **Threads in WebAssembly**, with a rayon thread pool in Web Workers. It needs COOP/COEP.
- **A build-time Tailwind plugin**, so that the browser does not load Tailwind.
- **Guides for edge runtimes, tested in CI.** For example, Cloudflare Workers limit the script
  size (3 MB compressed on the free plan, 10 MB on paid plans) and the CPU time per request.

## Decisions still open

Each has a recommendation.

1. **Panics.**
   - (a) **Stable toolchain and `panic = "abort"`, with E1 and recovery after a trap.**
     Recommended:
     - no nightly toolchain in the release pipeline;
     - a 5% smaller module;
     - counting the paint graph also bounds colour glyph work, whatever the painter is called
       with.
   - (b) **Nightly, with `-Zbuild-std` and WebAssembly exception handling.**
     - Native behaviour without E1.
     - But it puts a nightly toolchain and an unstable target feature in every release.
2. **Binding.** A hand-written ABI and wrapper (recommended), or wasm-bindgen.
3. **Entry point.**
   - `@inkoxide/render/browser`. Recommended: it is explicit, and a server bundle never picks
     it by accident.
   - Or a `browser` export condition on `@inkoxide/render`.
4. **Optimisation level.**
   - `opt-level = 3`. Recommended, for speed: 2.16 MB with brotli before E4 and wasm-opt.
   - Or `"s"`: 11% smaller, 16% slower.
5. **Parity.**
   - Allow last-digit differences where dependencies use trigonometry, with the fixtures listed
     and tested (recommended).
   - Or make them exact, which needs patched dependencies.
6. **Default fonts.**
   - TTF, fetched per document. Recommended: the same bytes as Node.
   - Or WOFF2: a smaller download, but other bytes.
7. **Edge runtimes.** Out of scope (decided): browsers and Node only.
