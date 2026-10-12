#!/usr/bin/env bash
# Runs the fuzz targets (cargo-fuzz, nightly) for SECONDS each, in
# parallel: render_tree (structure-aware, 3 workers), render_json (raw
# bytes seeded with fixtures/docs), render_svg (raw bytes as an image,
# seeded with fixtures/images), render_svg_tree (structure-aware SVG,
# 2 workers) and register_font (raw bytes as a font, also as WOFF and
# WOFF2, seeded with fixtures/fonts), or only the TARGETs named. Fails if
# anything crashes, times out or runs out of memory; findings land in
# fuzz/artifacts/.
#
# Without a sanitizer: the core crate has no unsafe code, and what the
# targets look for (panics, hangs, memory use) is what AddressSanitizer's
# own memory and time would blur. FUZZ_TOOLCHAIN picks the nightly
# toolchain (default: nightly).
#
#   scripts/fuzz.sh                       # every target, 60 s
#   scripts/fuzz.sh 3600                  # a one-hour run
#   scripts/fuzz.sh 600 render_svg_tree   # one target
set -euo pipefail
cd "$(dirname "$0")/../fuzz"

secs="${1:-60}"
shift || true
targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
  targets=(render_tree render_json render_svg render_svg_tree register_font)
fi
limits=(-timeout=10 -rss_limit_mb=2048)
fuzz=(cargo "+${FUZZ_TOOLCHAIN:-nightly}" fuzz)
build=(-O --sanitizer none)

# Seeds a target's corpus and prints its libFuzzer flags.
flags() {
  mkdir -p "corpus/$1"
  case $1 in
    render_tree)
      echo -fork=3 -ignore_crashes=1 -ignore_timeouts=1 -ignore_ooms=1 -max_len=8192 ;;
    render_json)
      cp ../fixtures/docs/*.json corpus/render_json/
      echo -dict=render_json.dict ;;
    render_svg)
      cp ../fixtures/images/* corpus/render_svg/
      echo -dict=render_svg.dict ;;
    render_svg_tree)
      echo -fork=2 -ignore_crashes=1 -ignore_timeouts=1 -ignore_ooms=1 -max_len=4096 ;;
    register_font)
      cp ../fixtures/fonts/*.ttf ../fixtures/fonts/*.otf ../fixtures/fonts/woff/* corpus/register_font/
      echo -max_len=1048576 ;;
    *)
      echo "unknown fuzz target: $1" >&2
      return 1 ;;
  esac
}

mkdir -p logs artifacts
for t in "${targets[@]}"; do
  "${fuzz[@]}" build "${build[@]}" "$t" >/dev/null 2>&1 || "${fuzz[@]}" build "${build[@]}" "$t"
done

before=$(find artifacts -type f | wc -l)
pids=()
for t in "${targets[@]}"; do
  read -ra args <<<"$(flags "$t")"
  "${fuzz[@]}" run "${build[@]}" "$t" -- "${args[@]}" -max_total_time="$secs" \
    "${limits[@]}" >"logs/$t.log" 2>&1 &
  pids+=($!)
done
status=0
for pid in "${pids[@]}"; do
  wait "$pid" || status=1
done
after=$(find artifacts -type f | wc -l)

# Statistics only: the logs show the inputs found.
for t in "${targets[@]}"; do
  echo "$t: $(grep -a -E '^#[0-9]+' "logs/$t.log" | tail -1 || true)"
done
if [[ $after -gt $before || $status -ne 0 ]]; then
  echo "fuzz findings: $((after - before)) new in fuzz/artifacts (details in fuzz/logs)"
  exit 1
fi
echo "fuzz: no findings in ${secs}s per target"
