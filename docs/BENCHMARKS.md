# Benchmarks

`cargo bench -p inkoxide --bench render` (criterion, release profile with
fat LTO). Machine: cloud dev container, 4 vCPU Intel Xeon @ 2.8 GHz —
slower than the M-series / 8-core targets in the brief (§9), so treat
absolute numbers as upper bounds.

| Milestone | Fixture | Pages | Median | Notes |
|---|---|---|---|---|
| M1 | `empty` | 1 | 13 µs | |
| M1 | `boxes-nested` | 1 | 0.60 ms | ~60 boxes, borders, radii; includes Flate level 6 |
| M2 | `invoice` | 1 | 2.4 ms | 2 fonts embedded in full (414 KB); target < 3 ms |
| M2 | `text-wrapping` | 1 | 3.7 ms | 5 fonts embedded, ~20 paragraphs |
| M2 | `text-justify-hyphenate-ro` | 1 | 2.5 ms | |

| M4 | `table-dump-500p` (generated, `scripts/fixtures/table_dump.py`) | 435 | 4.4 s | 20k rows × 5 cells; target < 2.5 s, see below |

M4 stage breakdown for the table dump (single run, release): parse 0.48 s
(27 MB JSON), layout 1.85 s (one Taffy subtree per cell, two to three
times per cell), paginate 0.02 s, paint 1.19 s, PDF 0.51 s. The planned
M6 work targets layout (fast path for single-text cells, cheaper per-cell
Taffy trees) and paint (parallel per page, ADR to come).

M2 numbers are `inkoxide bench --iterations 50` medians (release). The
first render of a process is slower (~45 ms here) because each font program
is Flate-compressed once and then cached (ADR-017).

## M6/M7 (current)

`inkoxide render` / `inkoxide bench` on the same 4-vCPU container
(release, fat LTO, font subsetting on). "t4" is 4 worker threads, the
most this machine has; the brief's multi-thread column assumes 8.

| Document | Single-thread | 4 threads | Target (§9) |
|---|---|---|---|
| `invoice` (1 page, median of 200) | 2.9 ms (min 2.5) | 3.0 ms | < 3 ms |
| `report-100p` (median of 10) | 318 ms | 158 ms | < 500 ms / < 200 ms (8) |
| `table-dump-500p` (435 pages, 27 MB JSON) | 2.59 s | 1.51 s | < 2.5 s / < 1 s (8) |
| Cold start, `invoice` with 2 fonts (first render in process) | 5–8 ms | | < 50 ms |
| Peak RSS, table dump | 213 MB | 209 MB | < 300 MB |

- The table dump is about 4% over the single-thread target on this
  machine, which is slower than the brief's reference hardware. It was
  4.4 s at M4.
  The 4-thread time scales as expected but cannot reach the 8-core < 1 s
  target here.
- Peak RSS fell from 306 MB to ~210 MB with interned node styles
  (ADR-040). Before that, `Style` (560 bytes) was stored inline in every
  node.
- The cold start (the first render in a process) includes font
  subsetting and is far below the ~45 ms measured at M2. The
  whole-process wall time, including exec and font loading, is ~35 ms.
- Generated inputs: `report-100p` and `table-dump-500p` come from
  `scripts/fixtures/` into `target/generated/`.
- Times vary ±10% between runs on this shared machine.

## Targets (§9)

| Document | Single-thread | Multi-thread (8) |
|---|---|---|
| 1-page invoice | < 3 ms | — |
| 100-page report | < 500 ms | < 200 ms |
| 500-page table dump | < 2.5 s | < 1 s |
| Cold start (2 fonts) | < 50 ms | — |
| Peak RSS, 500 pages | < 300 MB | |
