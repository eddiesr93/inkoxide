#!/usr/bin/env bash
# Stages and publishes @inkoxide/tailwind, @inkoxide/react and @inkoxide/render to npm. The
# release workflow runs it after publishing the engine packages.
#   scripts/release/publish.sh            # publish
#   scripts/release/publish.sh --dry-run  # stage and run npm publish --dry-run
#
# Order: stage.mjs --list (tailwind, react, render; each depends only on the ones before it).
# A name@version already on the registry is skipped, so a failed run can simply be repeated.
# Auth: in GitHub Actions, npm trusted publishing (OIDC, with provenance); locally, the user
# `npm login` set up, with 2FA in the browser.
set -euo pipefail
cd "$(dirname "$0")/../.."

dry_run=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    -h | --help)
      sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "publish: unknown argument: $arg (expected --dry-run)" >&2
      exit 2
      ;;
  esac
done

registry="https://registry.npmjs.org/"
# In stage.mjs's order. Read loops, not mapfile: macOS ships bash 3.2.
packages=()
while IFS= read -r line; do
  [[ -n $line ]] && packages+=("$line")
done < <(node scripts/release/stage.mjs --list)

if [[ -n "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ]]; then
  echo "publish: GitHub Actions; npm uses trusted publishing"
elif [[ $dry_run == 0 ]]; then
  who="$(npm whoami --registry "$registry" 2>/dev/null)" || {
    echo "publish: not logged in to npm. Run: npm login" >&2
    exit 1
  }
  echo "publish: logged in as $who; npm will ask for 2FA in the browser"
fi

node scripts/release/stage.mjs

# Prints "yes" when name@version is on the registry, "no" when it is not; fails on anything else.
published() {
  node scripts/release/registry.mjs "$1"
}

# Every registry dependency of the staged packages (the engine packages in particular) must
# already be published, or the published packages could not be installed.
missing_deps=0
deps=()
while IFS= read -r line; do
  [[ -n $line ]] && deps+=("$line")
done < <(node -e '
  const own = new Set(process.argv.slice(1).map((p) => `@inkoxide/${p}`));
  const all = new Set();
  for (const p of process.argv.slice(1)) {
    const m = require(`./tmp/release/${p}/package.json`);
    for (const [n, v] of Object.entries(m.dependencies ?? {})) if (!own.has(n)) all.add(`${n}@${v}`);
  }
  console.log([...all].join("\n"));
' "${packages[@]}")
for dep in ${deps[@]+"${deps[@]}"}; do
  [[ $dep == @inkoxide/schema@* || $dep == inkoxide@* ]] || continue
  state="$(published "$dep")"
  # Just published by the engine release: npm takes a few minutes to list a new version.
  for _ in $(seq 1 30); do
    [[ $state == yes || $dry_run == 1 ]] && break
    echo "publish: waiting for $dep to appear on the registry"
    sleep 20
    state="$(published "$dep")"
  done
  if [[ $state == no ]]; then
    echo "publish: dependency $dep is not on the registry; publish the engine packages first." >&2
    missing_deps=1
  fi
done
if [[ $missing_deps == 1 && $dry_run == 0 ]]; then exit 1; fi

for pkg in "${packages[@]}"; do
  dir="tmp/release/$pkg"
  name="$(node -p "require('./$dir/package.json').name")"
  version="$(node -p "require('./$dir/package.json').version")"
  state="$(published "$name@$version")"
  if [[ $state == yes ]]; then
    echo "publish: $name@$version is already on the registry; skipping"
    continue
  fi
  args=(publish "./$dir" --access public --registry "$registry")
  [[ $dry_run == 1 ]] && args+=(--dry-run)
  echo "publish: npm ${args[*]}"
  npm "${args[@]}"
done
echo "publish: done$([[ $dry_run == 1 ]] && echo ' (dry run, nothing uploaded)')"
