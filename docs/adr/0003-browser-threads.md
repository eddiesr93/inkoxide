# 3. Threads in the browser

Status: accepted, 2026-10-17. Supersedes the decision **One thread** of
[ADR 0002](0002-browser-rendering.md), and its rejection of a nightly toolchain with `-Zbuild-std`
for the module with threads.

## Context

[ADR 0002](0002-browser-rendering.md) shipped the engine as WebAssembly on one thread: threads
need shared memory, a nightly toolchain and cross-origin isolation. Natively, shaping, painting,
content streams and compression run in parallel (about 68% of a render's instructions), and a
98-page report renders 2.35× faster on 4 threads and 3.3× faster on 8.

- The standard library shipped for `wasm32-unknown-unknown` is built without `atomics` and
  `bulk-memory`, and `-C target-feature` does not rebuild it. Rebuilding it takes `-Zbuild-std`,
  which is nightly only.
- `wasm32-wasip1-threads` has a prebuilt standard library with threads, on stable. But its module
  imports WASI, which a page must provide, and its maths come from wasi-libc, not from the libm
  port the one-thread module uses.
- WebAssembly cannot start a thread. A page starts Web Workers, which share a
  `WebAssembly.Memory` only where `SharedArrayBuffer` exists: on a page that is cross-origin
  isolated (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy:
  require-corp`). A page's main thread cannot wait on shared memory, and a worker cannot start
  while the thread that created it waits.
- The standard library's allocator for WebAssembly takes one spin lock for every allocation.

## Decision

- **Two modules in `@inkoxide/wasm`.** `inkoxide.wasm` is the one-thread module, which behaves
  as before. A
  second one, `inkoxide-threads.wasm`, imports shared memory and cannot run without it: a page
  downloads one of the two. `init(source, { threads })` loads the module with threads, and
  `@inkoxide/render/browser` asks for `'auto'` threads by default: the number of cores, at most 8,
  on an isolated page, and 1 elsewhere. Where shared memory cannot be had, `init` loads the
  one-thread module, and where the pool's workers cannot start it renders on one thread;
  `threadCount()` says what it got. An explicit size may go up to 256, as natively.
- **`wasm32-unknown-unknown`, rebuilt on a pinned nightly.** `scripts/build-wasm.sh --threads`
  builds the standard library with `-Zbuild-std` on `nightly-2026-07-09` (with its `rust-src`;
  `scripts/build-wasm.sh --toolchain` prints it), with `+atomics,+bulk-memory`. It is the target and the maths of the one-thread module, and its
  output is the same, byte for byte, which `scripts/wasm-parity.mjs --threads` checks on every
  fixture, the fuzz seeds and the documents at the limits, with no pool and with pools of 3 and 8.
  The nightly builds only this module. `wasm32-wasip1-threads` was not tried: it needs a WASI
  shim and other maths. wasm-bindgen-rayon needs the glue ADR 0002 rejected.
- **A pool of our own, in the binding.** The module imports only its memory (16 MiB at first, at
  most 2 GiB) and exports its stack pointer and thread-local setup. For each pool thread, the
  render thread allocates a block (a 4 MiB stack, then the thread's thread-locals), and a worker
  (`thread.js`) instantiates the module on the shared memory, sets its stack pointer and
  thread-locals in that block, and waits on a queue. `ink_threads_start` builds rayon's global
  pool with a spawn handler that queues each thread instead of spawning it. A render that asks for
  one thread runs on the render thread; any other uses the pool. The engine itself does not
  change: the binding gives it rayon's global pool (`threads: None`). Unsafe code stays in
  `crates/wasm`.
- **An allocator with a cache per thread.** On the standard allocator's spin lock, the report got
  slower with each thread past two (311 ms on 8 threads against 117 ms on one, in Node). Each
  thread now keeps the small blocks it frees (up to 1 KiB, by 16-byte sizes, at most 32 KiB of
  each size) and allocates from them without the lock. Only the module with threads has it.
- **A bug on a thread ends the engine.** Both modules keep `panic = "abort"`. A trap on the
  render thread drops its instance and its pool: the next call gets a new instance on new memory,
  and a new pool once its workers have started. Until then renders run on one thread, those that
  ask for more too, and where the old memory is not freed yet the new one may have a smaller
  maximum. A trap on a pool thread leaves the render thread
  waiting for good, and it cannot hear of it: the pool thread posts on a `BroadcastChannel`
  (`trapChannel`), and `@inkoxide/render/browser`, which listens, ends the worker and fails the
  render with `INTERNAL`, as a `signal` would. Without a listener, `timeoutMs` still ends it.

## Consequences

- On an isolated page the engine renders the 98-page report 2.2× faster on 4 threads and 2.9× on
  8 (in Node); a React document gains less, since building its tree in JavaScript stays on one
  thread (1.3× for the 43-page report in Chromium and WebKit). One-page documents gain nothing.
- Few pages are isolated: COOP and COEP stop cross-origin resources that do not allow it. The
  one-thread module stays the main path.
- The release builds the module with threads on a nightly, which may later fail to build the
  standard library: the nightly is pinned, and moving it is a change of its own.
- WebKit refuses to start a worker again from its cache when a 304 response lacks the COEP
  header: servers must send it on every response.
- Memory is shared and never shrinks; each pool thread holds a stack, a shaper and its cache of
  blocks.

## Alternatives

- One thread, as ADR 0002 decided: no nightly and no second module, but the browser keeps none of
  the native gain, 2.2× to 2.9× on a long report.
- `wasm32-wasip1-threads` on stable: a WASI shim in every page, and wasi-libc's maths, so the output
  would differ from the one-thread module's. Not tried.
- wasm-bindgen-rayon: the generated glue ADR 0002 rejected, and workers of its own outside the
  binding.
- A worker per render, each with its own instance and no shared memory: renders run side by side,
  but one document renders no faster, and each instance holds its own fonts and memory.
- The standard allocator, or one with a lock around all of it: past two threads the report got
  slower with each thread.
- A `MessagePort` from each pool thread to the page for traps: each would have to be passed through
  the render worker, whose thread is the one blocked; a `BroadcastChannel` is found by its name, a
  string in `init`'s options.
