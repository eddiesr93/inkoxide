# Benchmarks

`cargo bench -p inkoxide --bench render` (criterion, release profile with
fat LTO). Machine: cloud dev container, 4 vCPU Intel Xeon @ 2.8 GHz —
slower than the M-series / 8-core targets in the brief (§9), so treat
absolute numbers as upper bounds.

| Milestone | Fixture | Pages | Median | Notes |
|---|---|---|---|---|
| M1 | `empty` | 1 | 13 µs | |
| M1 | `boxes-nested` | 1 | 0.60 ms | ~60 boxes, borders, radii; includes Flate level 6 |

## Targets (§9)

| Document | Single-thread | Multi-thread (8) |
|---|---|---|
| 1-page invoice | < 3 ms | — |
| 100-page report | < 500 ms | < 200 ms |
| 500-page table dump | < 2.5 s | < 1 s |
| Cold start (2 fonts) | < 50 ms | — |
| Peak RSS, 500 pages | < 300 MB | |
