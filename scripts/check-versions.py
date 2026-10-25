#!/usr/bin/env python3
"""Checks the lockstep versions (CHANGELOG.md): inkoxide, @inkoxide/wasm,
@inkoxide/render, @inkoxide/tree, @inkoxide/react, @inkoxide/vue,
@inkoxide/svelte, @inkoxide/angular, @inkoxide/tailwind and @inkoxide/style
share one version,
and so do the Rust crates (the workspace version and its exact pin of the
inkoxide crate), which the inkoxide package ships. @inkoxide/schema has
its own version.

    python3 scripts/check-versions.py
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCKSTEP = ["inkoxide", "wasm", "render", "tree", "react", "vue", "svelte", "angular", "tailwind", "style"]


def main() -> int:
    versions = {}
    for d in LOCKSTEP:
        pkg = json.loads((ROOT / "packages" / d / "package.json").read_text())
        versions[pkg["name"]] = pkg["version"]
    # Line-based (tomllib needs Python 3.11): `version = "…"` in
    # [workspace.package] and `inkoxide = { …, version = "=…" }`.
    cargo = (ROOT / "Cargo.toml").read_text()
    package = re.search(r'^\[workspace\.package\][^\[]*?^version = "([^"]+)"', cargo, re.M | re.S)
    pin = re.search(r'^inkoxide = \{[^}]*version = "=([^"]+)"', cargo, re.M)
    if not package or not pin:
        print("Cargo.toml: no workspace version or inkoxide pin", file=sys.stderr)
        return 1
    versions["Cargo.toml workspace.package"] = package[1]
    versions["Cargo.toml inkoxide dependency"] = pin[1]

    expected = versions["inkoxide"]
    wrong = {k: v for k, v in versions.items() if v != expected}
    if wrong:
        for k, v in wrong.items():
            print(f"{k} is at {v}, inkoxide at {expected}", file=sys.stderr)
        print("the lockstep packages must share one version (CHANGELOG.md)", file=sys.stderr)
        return 1
    print(f"lockstep version {expected}: {', '.join(versions)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
