#!/usr/bin/env bash
# Headline benchmark: renders fixtures/docs/report-100p-realistic.json
# (98 pages, 40 tables, header and footer) 10 times with the release CLI
# and prints the median in milliseconds. Extra arguments go to
# `inkoxide bench`, e.g. `scripts/bench-headline.sh --threads 1`.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q --release -p inkoxide-cli
out=$(target/release/inkoxide bench fixtures/docs/report-100p-realistic.json \
  --fonts fixtures/fonts --iterations 10 "$@")
median=$(sed -n 's/.*median \([0-9.]*\) ms.*/\1/p' <<<"$out")
[ -n "$median" ] || { echo "unexpected output: $out" >&2; exit 1; }
echo "$out" >&2
printf 'report-100p-realistic: median %.0f ms\n' "$median"
