#!/usr/bin/env bash
# Runs both fuzz targets (cargo-fuzz, nightly) for SECONDS each, in
# parallel: render_tree (structure-aware, 3 workers) and render_json (raw
# bytes seeded with fixtures/docs). Fails if anything crashes, times out
# or runs out of memory; findings land in fuzz/artifacts/.
#
#   scripts/fuzz.sh          # 60 s
#   scripts/fuzz.sh 3600     # the M6 acceptance run (1 h)
set -euo pipefail
cd "$(dirname "$0")/../fuzz"

secs="${1:-60}"
limits=(-timeout=10 -rss_limit_mb=2048)
mkdir -p corpus/render_tree corpus/render_json logs artifacts
cp ../fixtures/docs/*.json corpus/render_json/
cargo +nightly fuzz build -O >/dev/null 2>&1 || cargo +nightly fuzz build -O

before=$(find artifacts -type f 2>/dev/null | wc -l)
cargo +nightly fuzz run -O render_tree -- -fork=3 -ignore_crashes=1 \
  -ignore_timeouts=1 -ignore_ooms=1 -max_len=8192 -max_total_time="$secs" \
  "${limits[@]}" >logs/render_tree.log 2>&1 &
tree=$!
cargo +nightly fuzz run -O render_json -- -dict=render_json.dict \
  -max_total_time="$secs" "${limits[@]}" >logs/render_json.log 2>&1 &
json=$!
status=0
wait $tree || status=1
wait $json || status=1
after=$(find artifacts -type f 2>/dev/null | wc -l)

grep -a '^#' logs/render_tree.log | tail -1 || true
grep -a 'Done' logs/render_json.log || true
if [[ $after -gt $before || $status -ne 0 ]]; then
  echo "fuzz findings (see fuzz/artifacts, fuzz/logs):"
  find artifacts -type f 2>/dev/null | head
  exit 1
fi
echo "fuzz: no findings in ${secs}s per target"
