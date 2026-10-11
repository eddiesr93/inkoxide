# Benchmarks

`@inkoxide/render` compared with `@react-pdf/renderer` on the same documents, with the same
data, font (Inter), page size and page count. The results are in
[docs/BENCHMARKS.md](../docs/BENCHMARKS.md).

| File | What it is |
|---|---|
| `invoice-inkoxide.tsx` | The invoice in `@inkoxide/render`, styled with `className` |
| `invoice-react-pdf.tsx` | The same invoice in `@react-pdf/renderer`, styled with `StyleSheet.create` |
| `invoice-react-pdf-tailwind.tsx` | The same invoice in `@react-pdf/renderer` with `react-pdf-tailwind` |
| `report.tsx` | A 43-page report written once against the primitives both libraries share |
| `bench.tsx` | Warm renders (median), one-thread renders and cold starts in fresh processes |
| `cold.tsx` | One render in a fresh process, called by `bench.tsx` |

From the repository root, after `pnpm install` and `scripts/build-node.sh`:

```sh
pnpm --filter @inkoxide/benchmarks all     # writes the three invoice PDFs, to compare them
pnpm --filter @inkoxide/benchmarks bench   # prints the timings
```
