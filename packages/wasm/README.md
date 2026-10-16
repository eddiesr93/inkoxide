# @inkoxide/wasm

The Inkoxide PDF engine built as WebAssembly: a declarative document tree
(JSON) in, PDF bytes out, in browsers. The same engine, limits and error codes
as the native [`inkoxide`](https://www.npmjs.com/package/inkoxide) package,
with the same functions, plus `init` and `clearCaches`.

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

- **One thread.** Renders run on the calling thread, `render` included: in a
  page, render in a Web Worker. `threads` may only be `1`.
- **JSON only.** `format: 'msgpack'` is a `PARSE_ERROR`.
- **`init(source?)`** takes a URL, a `Response`, the bytes or a compiled
  `WebAssembly.Module`. It needs no cross-origin isolation; a Content Security
  Policy needs `script-src 'wasm-unsafe-eval'`.
- **A bug in the engine** (`INTERNAL`) ends its instance: the next call runs in
  a new one, with the fonts and hyphenation patterns registered so far, which
  the package keeps a copy of.
- **Memory** grows and is not given back. `clearCaches()` empties the caches
  renders share, for later renders to reuse the memory.
- **Fonts.** A WOFF2 font gives other PDF bytes than its TrueType file.

Options, defaults and error codes are typed in `index.d.ts` and described in
the [API reference](https://github.com/eddiesr93/inkoxide/blob/main/docs/API.md).
