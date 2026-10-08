#!/usr/bin/env bash
# Builds the native Node binding for the host platform into
# npm/inkoxide.<platform>.node (what npm/index.js loads first), which makes
# npm/ a complete package: install it with `pnpm add file:<repo>/npm`.
#   scripts/build-node.sh            # release
#   scripts/build-node.sh --debug
set -euo pipefail
cd "$(dirname "$0")/.."

profile=release
[[ "${1:-}" == "--debug" ]] && profile=debug
if [[ $profile == release ]]; then
  cargo build -p inkoxide-node --release
else
  cargo build -p inkoxide-node
fi

triple=$(node -e '
  const { platform, arch } = process;
  let t = `${platform}-${arch}`;
  if (platform === "linux") {
    const r = process.report.getReport();
    t += r.header.glibcVersionRuntime ? "-gnu" : "-musl";
  } else if (platform === "win32") t += "-msvc";
  console.log(t);')
case "$(uname -s)" in
  Darwin) lib=libinkoxide_node.dylib ;;
  MINGW*|MSYS*|CYGWIN*) lib=inkoxide_node.dll ;;
  *) lib=libinkoxide_node.so ;;
esac
cp "target/$profile/$lib" "npm/inkoxide.$triple.node"
echo "npm/inkoxide.$triple.node"

# The package must ship the loader, the types and the binary.
packed=$(cd npm && npm pack --dry-run --json 2>/dev/null)
for f in index.js index.d.ts README.md package.json "inkoxide.$triple.node"; do
  grep -q "\"path\": \"$f\"" <<<"$packed" || { echo "npm/ package is missing $f" >&2; exit 1; }
done
