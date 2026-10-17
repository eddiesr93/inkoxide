#!/usr/bin/env bash
# Builds the WebAssembly engine into packages/wasm/inkoxide.wasm (what its
# index.js loads by default), and copies the built-in hyphenation patterns it
# fetches into packages/wasm/hyphenation/, which makes packages/wasm a
# complete package.
#   scripts/build-wasm.sh
#   scripts/build-wasm.sh --threads   # packages/wasm/inkoxide-threads.wasm instead
#
# The build with threads uses shared memory, which the standard library
# shipped for wasm32-unknown-unknown is not built for: it rebuilds the
# standard library, with -Zbuild-std, on the nightly below, which needs its
# rust-src component (docs/adr/0003-browser-threads.md):
#   rustup toolchain install nightly-2026-07-09 --profile minimal --component rust-src
set -euo pipefail
cd "$(dirname "$0")/.."

NIGHTLY=nightly-2026-07-09

if [[ "${1:-}" == "--threads" ]]; then
  # rustc warns, once per crate, that the atomics feature is unstable.
  RUSTFLAGS="-C target-feature=+atomics,+bulk-memory" cargo "+$NIGHTLY" build -p inkoxide-wasm \
    --target wasm32-unknown-unknown --profile wasm --locked \
    -Zbuild-std=panic_abort,std --target-dir target/wasm-threads
  out=packages/wasm/inkoxide-threads.wasm
  cp target/wasm-threads/wasm32-unknown-unknown/wasm/inkoxide_wasm.wasm "$out"
else
  cargo build -p inkoxide-wasm --target wasm32-unknown-unknown --profile wasm --locked
  out=packages/wasm/inkoxide.wasm
  cp target/wasm32-unknown-unknown/wasm/inkoxide_wasm.wasm "$out"
fi
rm -rf packages/wasm/hyphenation
cp -R crates/core/hyphenation packages/wasm/hyphenation
rm packages/wasm/hyphenation/README.md
echo "$out ($(wc -c <"$out" | tr -d ' ') bytes)"
