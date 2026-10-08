#!/usr/bin/env bash
# Prints the pinned and latest stable crates.io versions of the
# unmaintained font crates and their successors (docs/DEPENDENCY_WATCH.md).
# Read-only; needs network access to crates.io.
set -euo pipefail
cd "$(dirname "$0")/.."

pinned() {
  sed -n "s/^$1 = \"=\\([^\"]*\\)\".*/\\1/p" Cargo.toml
}

latest() {
  curl -fsS -A 'inkoxide-dep-watch (https://github.com/eddiesr93/inkoxide)' \
    "https://crates.io/api/v1/crates/$1" |
    python3 -c 'import json,sys; c=json.load(sys.stdin)["crate"]; print(c["max_stable_version"] or c["max_version"], c["updated_at"][:10])'
}

printf '%-12s %-10s %-10s %s\n' crate pinned latest updated
for crate in rustybuzz ttf-parser harfrust read-fonts skrifa; do
  read -r version updated < <(latest "$crate")
  pin=$(pinned "$crate")
  printf '%-12s %-10s %-10s %s\n' "$crate" "${pin:--}" "$version" "$updated"
  [ "$crate" = ttf-parser ] && echo '-- successors'
  sleep 1 # crates.io asks for at most one request per second
done
