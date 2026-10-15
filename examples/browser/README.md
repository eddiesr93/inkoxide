# Inkoxide in the browser

A Vite page that renders an invoice with `@inkoxide/render/browser`: the engine is WebAssembly
(`@inkoxide/wasm`), in a Web Worker, and the PDF opens in the page.

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

## Tests

`pnpm test` builds the page, serves it with `vite preview`, and checks in Chromium, Firefox and
WebKit (`pnpm exec playwright install` first) that:

- the PDF rendered in the worker has the bytes the same invoice renders to in Node;
- the page is not cross-origin isolated, and logs no Content Security Policy violation;
- a render's `signal` ends its worker, and the next render runs in a new one.
