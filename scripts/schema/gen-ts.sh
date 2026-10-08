#!/usr/bin/env bash
# Regenerates npm-schema/ (TypeScript types + bundled JSON Schema) from
# schema/document.v1.json. The output is checked in; scripts/ci.sh fails
# when it is stale.
set -euo pipefail
cd "$(dirname "$0")"
pnpm install --frozen-lockfile --silent
node gen-ts.mjs
echo "npm-schema/index.d.ts, npm-schema/document.v1.json"
