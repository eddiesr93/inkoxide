#!/usr/bin/env bash
# Regenerates fixtures/expected/*.png from the current engine. Review the
# image changes in the PR before committing.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -q -p inkoxide --test fixtures
python3 scripts/visual-diff.py --update
