# @inkoxide/wasm

The Inkoxide PDF engine built as WebAssembly: a declarative document tree
(JSON) in, PDF bytes out, in browsers. The same engine, limits and error codes
as the native [`inkoxide`](https://www.npmjs.com/package/inkoxide) package,
with the same functions, plus `init`, `loadHyphenation` and `clearCaches`, and
one more render option, `deferredFonts`.

This page is for using the engine directly. To render React components in a
browser, in a Web Worker, use `@inkoxide/render/browser`, which runs this
package for you: see
[In the browser](https://github.com/eddiesr93/inkoxide/blob/main/packages/render/GUIDE.md#in-the-browser).

```sh
npm install @inkoxide/wasm
```

```js
import { init, registerFont, renderSync, InkoxideError } from '@inkoxide/wasm';

// Once: compiles `inkoxide.wasm`, which bundlers resolve next to this module.
await init();
registerFont('Inter-Regular', new Uint8Array(await (await fetch('/fonts/Inter-Regular.ttf')).arrayBuffer()));

const doc = {
  version: 1,
  fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
  pages: [{ size: { width: 595.28, height: 841.89 },
            children: [{ type: 'text', runs: [{ text: 'Hello', font: 'Inter-Regular', size: 24 }] }] }],
};

try {
  const pdf = renderSync(JSON.stringify(doc));
  const url = URL.createObjectURL(new Blob([pdf], { type: 'application/pdf' }));
} catch (e) {
  if (!(e instanceof InkoxideError)) throw e;
  console.error(e.code, e.path, e.message);
}
```

How it differs from `inkoxide`:

- **One thread, or a pool.** Renders run on the calling thread, `render`
  included: in a page, render in a Web Worker. `threads` may only be `1`,
  unless `init` started a pool (below).
- **JSON only.** `format: 'msgpack'` is a `PARSE_ERROR`.
- **Downloads on demand.** The module (about 1.4 MB with brotli) leaves out the built-in
  hyphenation patterns: `render` fetches a language's from `hyphenation/` next to the module the
  first time a document hyphenates text in it, and the fonts of its `deferredFonts` option (id →
  URL) that a document draws with, or needs as a fallback. `renderSync` cannot fetch, and fails
  with `NOT_LOADED`: call `loadHyphenation(['de'])` first, or use `render`.
- **`init(source?)`** takes a URL, a `Response`, the bytes or a compiled
  `WebAssembly.Module`. On one thread it needs no cross-origin isolation; a
  Content Security Policy needs `script-src 'wasm-unsafe-eval'`.
- **A bug in the engine** (`INTERNAL`) ends its instance: the next call runs in
  a new one, with the fonts and hyphenation patterns registered so far, which
  the package keeps a copy of.
- **Memory** grows and is not given back. `clearCaches()` empties the caches
  renders share, for later renders to reuse the memory.
- **Fonts.** A WOFF2 font gives other PDF bytes than its TrueType file.

**Threads.** In a worker of a page that is cross-origin isolated
(`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy:
require-corp`, on every response), or in Node, `init` can start a pool of
threads: it then loads `inkoxide-threads.wasm`, which shares its memory with
that many Web Workers (`thread.js`), and renders spread their pages over them,
for the same bytes:

```js
// A channel reaches every tab of the origin: a name of this engine's own.
const trapChannel = `inkoxide-trap-${crypto.randomUUID()}`;
await init(undefined, { threads: Math.min(navigator.hardwareConcurrency, 8), trapChannel });
threadCount(); // the pool's size, or 1 where memory cannot be shared
```

A render's `threads: 1` keeps to the calling thread. A bug on a pool thread
cannot end the call that waits for it: the thread posts `{ message }` on the
`BroadcastChannel` named `trapChannel`, and the page, listening, ends the
worker. `@inkoxide/render/browser` does all of this with its `threads` option.

**From a CDN**, give `init` the module with its
[Subresource Integrity](https://developer.mozilla.org/docs/Web/Security/Subresource_Integrity)
hash, so that the browser refuses any other bytes, and the folder of the
hyphenation patterns:

```js
const base = 'https://cdn.jsdelivr.net/npm/@inkoxide/wasm@<version>/';
await init(fetch(`${base}inkoxide.wasm`, { integrity: 'sha384-<hash of inkoxide.wasm>' }), {
  hyphenation: `${base}hyphenation/`,
});
```

With threads, each module has its hash. Fetch only the one the page will use;
the pool's `thread.js` must come from the page's origin, so the package is
better bundled:

```js
const threads = crossOriginIsolated ? 4 : 1;
const module = threads > 1
  ? fetch(`${base}inkoxide-threads.wasm`, { integrity: 'sha384-<hash of inkoxide-threads.wasm>' })
  : fetch(`${base}inkoxide.wasm`, { integrity: 'sha384-<hash of inkoxide.wasm>' });
await init(module, { threads, hyphenation: `${base}hyphenation/` });
```

The README published with each version on npm has its version and hashes here.

Options, defaults and error codes are typed in `index.d.ts` and described in
the [API reference](https://github.com/eddiesr93/inkoxide/blob/main/docs/API.md).
