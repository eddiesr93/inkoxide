# Inkoxide in the browser

A Vite page that renders an invoice with `@inkoxide/render/browser`: the engine is WebAssembly
(`@inkoxide/wasm`), in a Web Worker. `<PdfViewer>` shows the PDF in the page and
`<PdfDownloadLink>` downloads it.

```sh
scripts/build-wasm.sh           # packages/wasm/inkoxide.wasm
pnpm --filter @inkoxide/examples-browser dev
```

`vite.config.ts` shows what an application needs:

- `worker: { format: 'es' }`, for the engine's module worker;
- a Content Security Policy that allows `'wasm-unsafe-eval'` and workers of its own origin. No
  cross-origin isolation is needed.

`src/Invoice.tsx` imports from `@inkoxide/render`, as server code would; the config points that
at the browser entry.

`src/Letter.tsx` is a short letter in German, hyphenated, with a character Inter lacks. The page
does not show it; the tests render it. The engine fetches its German hyphenation patterns and
Noto Sans, the fallback font, only when a document needs them, as this one does: the invoice
needs neither.

## Tests

`pnpm test` builds the page, serves it with `vite preview`, and checks in Chromium, Firefox and
WebKit (`pnpm exec playwright install` first) that:

- the PDF rendered in the worker, and the file the download link saves, have the bytes the same
  invoice renders to in Node (`tests/render-node.tsx`);
- the letter has the bytes it renders to in Node, and rendering it fetches the German
  hyphenation patterns and Noto Sans;
- the page is not cross-origin isolated, and logs no Content Security Policy violation;
- a render's `signal` ends its worker, and the next render runs in a new one;
- a render that takes longer than its `timeoutMs` rejects with a `TimeoutError`, and the next
  render succeeds.
