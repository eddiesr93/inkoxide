#!/usr/bin/env bash
# Regenerates every committed generated file and fails if one changed: the
# one list both ci.yml and scripts/ci.sh check. Needs python3, and node with
# the workspace's dependencies installed (pnpm install). With --python-only,
# it checks only the files python3 writes, for a machine without pnpm. After
# a failure, the regenerated files are in place: review and commit them.
#
#   scripts/check-generated.sh
#   scripts/check-generated.sh --python-only
set -euo pipefail
cd "$(dirname "$0")/.."

python_only=0
case "${1:-}" in
  "") ;;
  --python-only) python_only=1 ;;
  *)
    echo "usage: scripts/check-generated.sh [--python-only]" >&2
    exit 2
    ;;
esac

python3 scripts/gen-schema-doc.py >/dev/null
python3 scripts/gen-fuzz-dict.py >/dev/null
python3 scripts/fixtures/report_realistic.py >/dev/null
python3 scripts/fixtures/emoji_fonts.py >/dev/null
generated=(
  docs/SCHEMA.md
  fuzz/render_json.dict
  fixtures/docs/report-100p-realistic.json
  fixtures/fonts/EmojiTest-COLR.ttf
  fixtures/fonts/EmojiTest-CBDT.ttf
  fixtures/fonts/EmojiTest-sbix.ttf
  fixtures/fonts/limits/colr-fanout.ttf
  fixtures/fonts/limits/colr-fanout-empty.ttf
)

if [[ $python_only == 0 ]]; then
  node scripts/gen-schema-types.mjs >/dev/null
  node scripts/gen-schema-validator.mjs
  pnpm exec tsx scripts/gen-tailwind-docs.ts >/dev/null
  generated+=(
    packages/schema/index.d.ts
    packages/react/src/tree/schema-validator.ts
    docs/TAILWIND_SUPPORT.md
  )
fi

git diff --exit-code --stat -- "${generated[@]}" || {
  echo "generated files are stale: commit the files scripts/check-generated.sh regenerated" >&2
  exit 1
}
if [[ $python_only == 0 ]]; then
  echo "generated files: up to date"
else
  echo "generated files that python3 writes: up to date (the others need pnpm)"
fi
