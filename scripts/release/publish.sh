#!/usr/bin/env bash
# Stages and publishes the three library packages to npm, from a local machine.
#   scripts/release/publish.sh                           # publish as the user `npm login` set up
#   NPM_TOKEN=... scripts/release/publish.sh             # publish with a token instead
#   scripts/release/publish.sh --dry-run                 # stage and run npm publish --dry-run
#
# Order: tailwind, react, render (each depends only on the ones before it).
# A name@version already on the registry is skipped, so a failed run can simply be repeated.
# Auth: without NPM_TOKEN, npm uses your login and asks for 2FA in the browser for each package.
# With NPM_TOKEN, it is referenced from a temporary npmrc as ${NPM_TOKEN} (npm expands it), so
# the token is never written to disk or printed.
set -euo pipefail
cd "$(dirname "$0")/../.."

dry_run=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    -h | --help)
      sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "publish: unknown argument: $arg (expected --dry-run)" >&2
      exit 2
      ;;
  esac
done

registry="https://registry.npmjs.org/"
packages=(tailwind react render)

if [[ -z "${NPM_TOKEN:-}" ]]; then
  if [[ $dry_run == 0 ]]; then
    who="$(npm whoami --registry "$registry" 2>/dev/null)" || {
      echo "publish: not logged in to npm. Run: npm login   (or set NPM_TOKEN)" >&2
      exit 1
    }
    echo "publish: logged in as $who; npm will ask for 2FA in the browser"
  fi
else
  npmrc="$(mktemp)"
  trap 'rm -f "$npmrc"' EXIT
  chmod 600 "$npmrc"
  # Single quotes: the file holds the literal ${NPM_TOKEN}, which npm expands at run time.
  # shellcheck disable=SC2016
  printf '%s\n' "registry=${registry}" '//registry.npmjs.org/:_authToken=${NPM_TOKEN}' >"$npmrc"
  export NPM_CONFIG_USERCONFIG="$npmrc"
  if [[ $dry_run == 0 ]]; then
    who="$(npm whoami --registry "$registry" 2>/dev/null)" || {
      echo "publish: npm rejected NPM_TOKEN (npm whoami failed)." >&2
      exit 1
    }
    echo "publish: authenticated as $who"
  fi
fi

node scripts/release/stage.mjs

# Prints "yes" when name@version is on the registry, "no" on a 404; fails on anything else.
published() {
  local spec="$1" out
  if out="$(npm view "$spec" version --registry "$registry" 2>&1)"; then
    [[ -n "$out" ]] && echo yes || echo no
  elif grep -q 'E404' <<<"$out"; then
    echo no
  else
    echo "publish: npm view $spec failed:" >&2
    echo "$out" >&2
    return 1
  fi
}

# Every registry dependency of the staged packages (the engine packages in particular) must
# already be published, or the published packages could not be installed.
missing_deps=0
# A read loop, not mapfile: macOS ships bash 3.2.
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
  if [[ $state == no ]]; then
    echo "publish: dependency $dep is not on the registry; publish it from the engine repo first." >&2
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
