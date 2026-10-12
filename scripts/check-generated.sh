#!/usr/bin/env bash
# Regenerates every committed generated file and fails if one changed: the
# one list both ci.yml and scripts/ci.sh check. Needs python3, and node with
# the workspace's dependencies installed (pnpm install). After a failure,
# the regenerated files are in place: review and commit them.
#
#   scripts/check-generated.sh
set -euo pipefail
cd "$(dirname "$0")/.."

node scripts/gen-schema-types.mjs >/dev/null
pnpm exec tsx scripts/gen-tailwind-docs.ts >/dev/null
python3 scripts/gen-schema-doc.py >/dev/null
python3 scripts/gen-fuzz-dict.py >/dev/null
python3 scripts/fixtures/report_realistic.py >/dev/null

generated=(
  packages/schema/index.d.ts
  docs/TAILWIND_SUPPORT.md
  docs/SCHEMA.md
  fuzz/render_json.dict
  fixtures/docs/report-100p-realistic.json
)
git diff --exit-code --stat -- "${generated[@]}" || {
  echo "generated files are stale: commit the files scripts/check-generated.sh regenerated" >&2
  exit 1
}
echo "generated files: up to date"
