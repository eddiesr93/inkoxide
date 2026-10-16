# Benchmarks

`@inkoxide/render` compared with `@react-pdf/renderer` on the same documents, with the same
data, font (Inter), page size and page count, on the server and in the browser. The results are in
[docs/BENCHMARKS.md](../docs/BENCHMARKS.md).

| File | What it is |
|---|---|
| `documents/` | The documents, written without Node so that the browser benchmark renders them too |
| `invoice-inkoxide.tsx` | The invoice in `@inkoxide/render`, styled with `className` |
| `invoice-react-pdf.tsx` | The same invoice in `@react-pdf/renderer`, styled with `StyleSheet.create` |
| `invoice-react-pdf-tailwind.tsx` | The same invoice in `@react-pdf/renderer` with `react-pdf-tailwind` |
| `report.tsx` | A 43-page report written once against the primitives both libraries share |
| `bench.tsx` | On the server: warm renders (median), one-thread renders and cold starts in fresh processes |
| `cold.tsx` | One render in a fresh process, called by `bench.tsx` |
| `browser/` | In the browser: a page per library, built with Vite, measured with Playwright in Chromium, WebKit and Firefox |

From the repository root, after `pnpm install`, `scripts/build-node.sh` and
`scripts/build-wasm.sh`:

```sh
pnpm --filter @inkoxide/benchmarks all             # writes the three invoice PDFs, to compare them
pnpm --filter @inkoxide/benchmarks bench           # prints the timings on the server
pnpm --filter @inkoxide/benchmarks bench:browser   # prints them in each browser
```

`bench:browser` needs Playwright's browsers (`pnpm --filter @inkoxide/benchmarks exec playwright
install`). For each browser it loads each library's page three times in a fresh context: the first
invoice from the start of navigation, the median of warm renders, the longest main-thread task
(Chromium) and what the page downloaded, with brotli. Use a release build of the native binding:
`scripts/ci.sh` leaves a debug build.
