#!/usr/bin/env bash
# Local CI: everything a PR must pass. No hosted CI runs for this repo;
# run this before pushing.
#
#   scripts/ci.sh           # full run
#   scripts/ci.sh --quick   # skip clippy/deny/visual diff
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
[[ "${1:-}" == "--quick" ]] && quick=1

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "rustfmt"
cargo fmt --all -- --check

if [[ $quick == 0 ]]; then
  step "clippy"
  cargo clippy --workspace --all-targets -- -D warnings
fi

step "tests"
cargo test --workspace

step "node binding (npm/)"
if command -v node >/dev/null; then
  scripts/build-node.sh --debug >/dev/null
  (cd npm && node --test test/*.test.js test/*.test.mjs) | grep -E '^# (pass|fail)'
else
  echo "node not installed; skipped"
fi

step "schema docs up to date"
python3 scripts/gen-schema-doc.py --check

step "schema conformance of fixtures"
python3 scripts/check-schema.py

step "fixture outputs (qpdf --check)"
if command -v qpdf >/dev/null; then
  for f in target/fixtures/*.pdf; do
    qpdf --check "$f" >/dev/null || { echo "qpdf failed: $f"; exit 1; }
  done
  echo "qpdf: ok"
else
  echo "qpdf not installed; skipped (apt install qpdf)"
fi

if [[ $quick == 0 ]]; then
  step "cargo-deny"
  if command -v cargo-deny >/dev/null; then
    cargo deny check
  else
    echo "cargo-deny not installed; skipped (cargo install cargo-deny)"
  fi

  if [[ -x scripts/visual-diff.sh ]]; then
    step "visual regression"
    scripts/visual-diff.sh
  fi
fi

step "all checks passed"
