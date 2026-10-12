#!/usr/bin/env python3
"""Generates fuzz/render_json.dict, the libFuzzer dictionary of the
render_json target, from packages/schema/document.v1.json: every property
name and string enum value, plus JSON syntax and fixture ids.

  scripts/gen-fuzz-dict.py          # rewrite fuzz/render_json.dict
  scripts/gen-fuzz-dict.py --check  # fail if fuzz/render_json.dict is stale
"""
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parent.parent
schema = json.loads((root / "packages/schema/document.v1.json").read_text())

# Not in the schema: ids of the fixture fonts and images, link targets and
# common values.
EXTRA_STRINGS = [
    "1fr",
    "50%",
    "Inter-Regular",
    "Inter-Bold",
    "logo",
    "photo",
    "#n0",
    "https://x",
]
# Bare tokens: JSON syntax and edge numbers.
EXTRA_TOKENS = ["{", "}", "[", "]", ":", ",", "null", "true", "false", "1e308", "-0", "0.5"]


def strings(node, found):
    """Collects property names, string enum values and string consts."""
    if isinstance(node, dict):
        for name, value in node.get("properties", {}).items():
            found.add(name)
            strings(value, found)
        for value in node.get("enum", []):
            if isinstance(value, str):
                found.add(value)
        if isinstance(node.get("const"), str):
            found.add(node["const"])
        for key, value in node.items():
            if key not in ("properties", "enum", "const", "description"):
                strings(value, found)
    elif isinstance(node, list):
        for value in node:
            strings(value, found)
    return found


def entry(token):
    """A dictionary entry: the token in double quotes, with `"` and `\\` escaped."""
    return '"' + token.replace("\\", "\\\\").replace('"', '\\"') + '"'


tokens = sorted(strings(schema, set()) | set(EXTRA_STRINGS))
lines = [
    "# libFuzzer dictionary for render_json: schema keys and enum values.",
    "# Generated from packages/schema/document.v1.json by scripts/gen-fuzz-dict.py.",
]
lines += [entry(json.dumps(t)) for t in tokens]
lines += [entry(t) for t in EXTRA_TOKENS]
text = "\n".join(lines) + "\n"

target = root / "fuzz/render_json.dict"
if "--check" in sys.argv:
    if not target.exists() or target.read_text() != text:
        print("fuzz/render_json.dict is stale: run scripts/gen-fuzz-dict.py")
        sys.exit(1)
    print("fuzz/render_json.dict: up to date")
else:
    target.write_text(text)
    print(f"wrote {target.relative_to(root)}")
