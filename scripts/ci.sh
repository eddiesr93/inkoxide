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
  cargo clippy --workspace --all-targets -- -D warnings
  cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
fi

step "tests"
# PDF/A outputs are checked with veraPDF when it is installed.
if [[ -z "${VERAPDF:-}" ]] && command -v verapdf >/dev/null; then
  export VERAPDF=verapdf
fi
[[ -n "${VERAPDF:-}" ]] || echo "verapdf not installed; PDF/A outputs not validated"
cargo test --workspace
cargo test --manifest-path fuzz/Cargo.toml --lib

step "node binding (packages/inkoxide)"
if command -v node >/dev/null; then
  scripts/build-node.sh --debug >/dev/null
  # The greps below read TAP; Node 23+ prints the spec reporter even when piped.
  (cd packages/inkoxide && node --test --test-reporter=tap test/*.test.js) |
    grep -E '^# (pass|fail)'
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
  node scripts/gen-schema-types.mjs
  pnpm exec tsx scripts/gen-tailwind-docs.ts
  git diff --exit-code -- packages/schema/index.d.ts docs/TAILWIND_SUPPORT.md || {
    echo "generated files are stale: run node scripts/gen-schema-types.mjs and"
    echo "pnpm exec tsx scripts/gen-tailwind-docs.ts, then commit the result"
    exit 1
  }
else
  echo "pnpm not installed; skipped (npm i -g pnpm)"
fi

step "schema docs up to date"
python3 scripts/gen-schema-doc.py --check

step "generated fixtures up to date"
python3 scripts/fixtures/report_realistic.py >/dev/null
git diff --exit-code -- fixtures/docs/report-100p-realistic.json || {
  echo "report-100p-realistic.json is stale: run scripts/fixtures/report_realistic.py and commit the result"
  exit 1
}
echo "report-100p-realistic.json: up to date"

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
