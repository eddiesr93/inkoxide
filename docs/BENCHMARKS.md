# Benchmarks

**Headline:** `scripts/bench-headline.sh` renders
`fixtures/docs/report-100p-realistic.json` (98 pages, 40 tables, two
fonts, a header, and a footer with page numbers) 10 times and prints the
median. On the 4-vCPU container described below, measured 2026-10-08:

| | Median |
|---|---|
| All cores (4) | **130 ms** (two runs: 125, 136) |
| `--threads 1` | **277 ms** (two runs: 280, 274) |

The brief's 100-page targets are < 500 ms single-threaded and < 200 ms
on 8 threads.

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

## Table dump, single thread (post-M7 profiling)

Time-boxed profiling pass on `table-dump-500p`, single thread (work
order T7). Stage wall times come from a temporary instrumented build
(three runs; ranges show the noise on this shared machine). Instruction
shares come from `valgrind --tool=callgrind` on a 2,000-row version of
the same table (`scripts/fixtures/table_dump.py 2000`), counted inside
`render` only. Instruction counts are deterministic, so they compare
experiments better than wall time does here.

| Stage | Wall time (20k rows) | Share of instructions | What it is |
|---|---|---|---|
| Parse | 0.25–0.45 s | 18.7% | serde_json over 27 MB, 100k styled nodes |
| Validate | 0.05–0.09 s | ~2% | |
| Shape | 0.44–0.58 s | 26.5% | rustybuzz on every distinct word (the word cache already reuses repeats); GPOS kerning of digit strings alone is 7% |
| Layout + paginate | 0.31–0.45 s | 5.4% | Taffy is already skipped for every cell (fast path, ADR-028); what remains is line breaking and the per-cell size cache |
| Paint | 0.36–0.39 s | ~9% | ops for 100k cells and their borders |
| Content streams | 0.15–0.19 s | ~6% | 19 MB of operators; float formatting (ryu) is 3% |
| Deflate (level 6) | 0.49–0.58 s | 26.6% | miniz_oxide, 19 MB in, 2.6 MB out |
| Write | < 0.01 s | | |

Experiments. Each one was kept only if the output stayed byte-identical
and the gain was at least 5%:

| Change | Instructions | Result |
|---|---|---|
| (a) Extend the simple-cell fast path to skip Taffy | n/a | Nothing to extend: every cell of this table already takes the fast path. Layout is 5% of the total |
| (b) Reuse the `Vec<Op>` buffer per worker (`map_init`) | −0.03% | Rejected; byte-identical |
| (c) Pre-size the content-stream buffer (48 bytes per op) | −0.07% | Rejected; byte-identical |
| Share shaping results between text nodes with identical runs and style | +0.3% | Rejected; byte-identical. The existing word cache already avoids reshaping, and building the content key costs as much as it saves |

Median single-thread wall time over these runs: 2.40–2.70 s (the M6
figure was 2.59 s). The ≤ 2.3 s goal is not met on this machine, and the
remaining cost is structural:

- **Deflate is a quarter of the work.** At level 6, miniz_oxide must
  compress 19 MB. A faster level or a different compressor changes the
  output bytes, so it is a product decision, not an optimisation.
  `compressionLevel` already lets callers trade size for speed.
- **Shaping unique strings.** 20k rows of IDs, names and totals are
  mostly distinct words, and each one goes through rustybuzz. This
  changes with the harfrust migration (docs/DEPENDENCY_WATCH.md), not
  before.
- **Parsing 27 MB of JSON.** This is serde_json's speed. MessagePack
  input is the existing lever.
- What is left (paint, layout, content streams) is about 20% of the work
  and spread over many small functions, with no hotspot above 3%.

The multi-thread time (1.5 s on 4 threads) parallelises shaping, paint,
content streams and deflate per page, which is where integrators should
look for throughput.

## Targets (§9)

| Document | Single-thread | Multi-thread (8) |
|---|---|---|
| 1-page invoice | < 3 ms | — |
| 100-page report | < 500 ms | < 200 ms |
| 500-page table dump | < 2.5 s | < 1 s |
| Cold start (2 fonts) | < 50 ms | — |
| Peak RSS, 500 pages | < 300 MB | |
