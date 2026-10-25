# Inkoxide in the browser

A Vite page that renders an invoice with `@inkoxide/react/browser`: the engine is WebAssembly
(`@inkoxide/wasm`), in a Web Worker. `<PdfViewer>` shows the PDF in the page and
`<PdfDownloadLink>` downloads it.

```sh
scripts/build-wasm.sh           # packages/wasm/inkoxide.wasm
scripts/build-wasm.sh --threads # packages/wasm/inkoxide-threads.wasm, for the isolated page
pnpm --filter @inkoxide/examples-browser dev
```

`vite.config.ts` shows what an application needs:

- `worker: { format: 'es' }`, for the engine's module worker;
- a Content Security Policy that allows `'wasm-unsafe-eval'` and workers of its own origin. No
  cross-origin isolation is needed; with `INKOXIDE_ISOLATED` set, `vite preview` serves the page
  cross-origin isolated (`isolated()`), and the engine renders on a pool of threads;
- `inkoxideTailwind()` from `@inkoxide/tailwind/plugin`, which puts a table of the page's classes
  in the bundle in place of Tailwind's compiler.

`src/Invoice.tsx` imports from `@inkoxide/react`, as server code would; the config points that
at the browser entry.

`src/Letter.tsx` is a short letter in German, hyphenated, with a character Inter lacks. The page
does not show it; the tests render it. The engine fetches its German hyphenation patterns and
Noto Sans, the fallback font, only when a document needs them, as this one does: the invoice
needs neither.

`vite.config.ts` also serves the build as a CDN would, for `tests/cdn.spec.ts`: with
`INKOXIDE_CDN=assets`, `vite preview` serves the scripts and assets on port 4181 with CORS, and
with `INKOXIDE_CDN=page`, the page on port 4182, cross-origin isolated, under a policy with
`worker-src blob:`. Playwright starts both.

## Tests

`pnpm test` builds the page, serves it with `vite preview`, and checks in Chromium, Firefox and
WebKit (after `scripts/build-node.sh`, both `scripts/build-wasm.sh` builds and
`pnpm exec playwright install`) that:

- the PDF rendered in the worker, and the file the download link saves, have the bytes the same
  invoice renders to in Node (`tests/render-node.tsx`);
- the letter has the bytes it renders to in Node, and rendering it fetches the German
  hyphenation patterns and Noto Sans;
- the page is not cross-origin isolated, and logs no Content Security Policy violation;
- a render's `signal` ends its worker, and the next render runs in a new one;
- a render that takes longer than its `timeoutMs` rejects with a `TimeoutError`, and the next
  render succeeds.

`tests/threads.spec.ts` serves the page again, cross-origin isolated, and checks that the engine
renders on a pool of threads with the bytes Node renders; that `signal` and `timeoutMs` stop a
render on the pool; and that a bug on a pool thread fails the render with `INTERNAL`. After each,
the next render succeeds.

`tests/cdn.spec.ts` serves the build again as a CDN would: its scripts and assets on port 4181
(`INKOXIDE_CDN=assets`, with CORS), and the page on port 4182 (`INKOXIDE_CDN=page`,
cross-origin isolated, under a policy with `worker-src blob:`). A page cannot start a worker
from a script of another origin, so the engine's worker and its pool's start from `blob:` URLs of
the page's origin; the test checks that, and that the engine renders on a pool of threads with
the bytes Node renders. Playwright starts both servers.
