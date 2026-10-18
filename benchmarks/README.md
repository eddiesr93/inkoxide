# Benchmarks

`@inkoxide/render` on an invoice and a 43-page report, in Inter, on the server and in the
browser. The results are in [docs/BENCHMARKS.md](../docs/BENCHMARKS.md).

| File | What it is |
|---|---|
| `documents/` | The documents, `invoice.tsx` and `report.tsx`, written without Node so that the browser benchmark renders them too |
| `invoice.json` | The invoice's data: supplier, client, items, currency and VAT rate |
| `logo.jpg` | The invoice's logo |
| `invoice.tsx` | The invoice, styled with `className`, with its logo read from disk; run directly, it writes `invoice.pdf` |
| `bench.tsx` | On the server: warm renders (median), one-thread renders and cold starts in fresh processes |
| `cold.tsx` | One render in a fresh process, called by `bench.tsx` |
| `browser/` | In the browser: a page (`index.html`, `main.tsx`) built with Vite and the `inkoxideTailwind()` build plugin, measured with Playwright in Chromium, WebKit and Firefox |

From the repository root, after `pnpm install`, `scripts/build-node.sh` and
`scripts/build-wasm.sh`:

```sh
pnpm --filter @inkoxide/benchmarks invoice         # writes the invoice PDF
pnpm --filter @inkoxide/benchmarks bench           # prints the timings on the server
pnpm --filter @inkoxide/benchmarks bench:browser   # prints them in each browser
```

`bench:browser` needs Playwright's browsers (`pnpm --filter @inkoxide/benchmarks exec playwright
install`). For each browser it loads the page three times in a fresh context: the first invoice
from the start of navigation, the median of warm renders, the longest main-thread task
(Chromium) and what the page downloaded, with brotli; then the warm report on pools of 4 and 8
threads, on the page, which it serves cross-origin isolated (after `scripts/build-wasm.sh
--threads`). Use a release build of the native binding:
`scripts/ci.sh` leaves a debug build.
