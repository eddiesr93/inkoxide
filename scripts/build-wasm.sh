#!/usr/bin/env bash
# Builds the WebAssembly engine into packages/wasm/inkoxide.wasm (what its
# index.js loads by default), and copies the built-in hyphenation patterns it
# fetches into packages/wasm/hyphenation/, which makes packages/wasm a
# complete package.
#   scripts/build-wasm.sh
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p inkoxide-wasm --target wasm32-unknown-unknown --profile wasm --locked
cp target/wasm32-unknown-unknown/wasm/inkoxide_wasm.wasm packages/wasm/inkoxide.wasm
rm -rf packages/wasm/hyphenation
cp -R crates/core/hyphenation packages/wasm/hyphenation
rm packages/wasm/hyphenation/README.md
echo "packages/wasm/inkoxide.wasm ($(wc -c <packages/wasm/inkoxide.wasm | tr -d ' ') bytes)"
