#!/usr/bin/env bash
# Every check a pull request must pass, plus the slower ones that the hosted
# workflow skips (visual regression, cargo-deny, a fuzz smoke).
#
#   scripts/ci.sh           # full run
#   scripts/ci.sh --quick   # skip clippy, cargo-deny, fuzzing and the visual diff
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
echo "platform: $(uname -sm)"
[[ "${1:-}" == "--quick" ]] && quick=1

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "rustfmt"
cargo fmt --all -- --check
cargo fmt --manifest-path fuzz/Cargo.toml -- --check

if [[ $quick == 0 ]]; then
  step "clippy"
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --locked -- -D warnings
fi

step "tests"
# PDF/A outputs are checked with veraPDF when it is installed.
if [[ -z "${VERAPDF:-}" ]] && command -v verapdf >/dev/null; then
  export VERAPDF=verapdf
fi
[[ -n "${VERAPDF:-}" ]] || echo "verapdf not installed; PDF/A outputs not validated"
# Every fixture output passes `qpdf --check` when qpdf is installed
# (crates/core/tests/fixtures.rs).
command -v qpdf >/dev/null || echo "qpdf not installed; fixture outputs not checked (apt install qpdf)"
command -v pdftotext >/dev/null ||
  echo "pdftotext not installed; extracted text not checked (apt install poppler-utils)"
cargo test --workspace --locked
cargo test --manifest-path fuzz/Cargo.toml --lib --locked

step "node binding (packages/inkoxide)"
if command -v node >/dev/null; then
  scripts/build-node.sh --debug >/dev/null
  # schema-parity.test.js imports ajv from the workspace dependencies.
  if command -v pnpm >/dev/null; then pnpm install --frozen-lockfile --silent; fi
  # The totals when the tests pass, the whole report when one fails. TAP:
  # Node 23+ prints the spec reporter even when piped.
  log=$(mktemp)
  if (cd packages/inkoxide && node --test --test-reporter=tap test/*.test.js) >"$log" 2>&1; then
    grep -E '^# (pass|fail)' "$log"
  else
    cat "$log"
    exit 1
  fi
else
  echo "node not installed; skipped"
fi

step "TypeScript packages (typecheck, lint, tests)"
if command -v pnpm >/dev/null; then
  pnpm install --frozen-lockfile --silent
  pnpm typecheck
  pnpm lint
  pnpm format:check
  NODE_ENV=test pnpm test

  step "generated files up to date"
  scripts/check-generated.sh
else
  echo "pnpm not installed; skipped (npm i -g pnpm)"

  step "generated files up to date (those python3 writes; the others need pnpm)"
  scripts/check-generated.sh --python-only
fi

step "schema conformance of fixtures"
python3 scripts/check-schema.py

if [[ $quick == 0 ]]; then
  step "cargo-deny"
  if command -v cargo-deny >/dev/null; then
    cargo deny check
  else
    echo "cargo-deny not installed; skipped (cargo install cargo-deny)"
  fi

  step "fuzz smoke (30 s per target)"
  if command -v cargo-fuzz >/dev/null && rustup toolchain list | grep -q nightly; then
    scripts/fuzz.sh 30
  else
    echo "nightly or cargo-fuzz missing; skipped (see docs/TESTING.md)"
  fi

  if [[ -x scripts/visual-diff.sh ]]; then
    step "visual regression"
    scripts/visual-diff.sh
  fi
fi

step "all checks passed"
