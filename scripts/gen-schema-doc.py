#!/usr/bin/env python3
"""Generates docs/SCHEMA.md from packages/schema/document.v1.json.

  scripts/gen-schema-doc.py          # rewrite docs/SCHEMA.md
  scripts/gen-schema-doc.py --check  # fail if docs/SCHEMA.md is stale
"""
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parent.parent
schema = json.loads((root / "packages/schema/document.v1.json").read_text())
defs = schema["$defs"]

INTRO = """# Document schema v1

> Generated from [`packages/schema/document.v1.json`](../packages/schema/document.v1.json) by
> `scripts/gen-schema-doc.py`. Do not edit by hand; edit the schema and
> regenerate. Behavioural notes live in the schema `description`s and in
> [ARCHITECTURE.md](ARCHITECTURE.md).

## Conventions

- **Units.** Every length is in PDF points (1/72 in). Percentages
  (`"50%"`) are allowed only where the type says `Length` or
  `LengthPercent`; they resolve against the containing block (width for
  padding and horizontal properties, as in CSS).
- **Colours.** `[r, g, b, a]` floats in `[0, 1]`, DeviceRGB.
- **Strictness.** Unknown properties are errors (`UNKNOWN_FIELD`), as are
  properties on the wrong node type (e.g. `runs` on a `box`) and text-only
  style properties on non-text nodes (`STYLE_NOT_APPLICABLE`).
- **Resolved input.** Nothing is inherited: every text run carries its own
  font, size and colour.
- **Defaults** follow CSS flexbox except `flexDirection`, which defaults to
  `column` (document flow is vertical).
- **Header/footer.** Laid out at the full page width: once, or again on
  every page when it shows page numbers. The header is painted at the top
  of every physical page (y = 0), the footer with its bottom edge on the
  bottom of the page. Use their padding to inset them and size the page
  margins so the flow does not overlap them.
- **Placeholders.** A run with `placeholder: "pageNumber" | "totalPages"`
  is filled in after pagination. In a header or footer, the text is laid
  out with each page's numbers, so it wraps and aligns at their width. In
  the flow, layout reserves the width of `999` in the run's font, since
  pagination depends on it; a longer number can overflow that space.
- **Images.** Intrinsic size is the pixel size at 96 dpi (1 px = 0.75 pt),
  like CSS. `fit` defaults to `fill`, like CSS `object-fit`, and
  `objectPosition` places a fitted image in its box like `object-position`
  (centred by default; what falls outside the box is clipped).

## Validation error codes

| Code | Meaning |
|---|---|
| `PARSE_ERROR` | Input is not well-formed JSON |
| `UNSUPPORTED_VERSION` | `version` is not `1` |
| `UNKNOWN_FIELD` | Property not allowed at this position |
| `MISSING_FIELD` | Required property missing |
| `INVALID_TYPE` | Wrong JSON type |
| `INVALID_VALUE` | Value not in the allowed set / wrong format |
| `OUT_OF_RANGE` | Number outside its allowed range |
| `DUPLICATE_ID` | Font id declared twice |
| `UNKNOWN_FONT` | Font id not declared in `fonts` |
| `FONT_NOT_REGISTERED` | Declared font has no registered bytes |
| `UNKNOWN_IMAGE` | Image `src` not provided in the render options |
| `INVALID_IMAGE` | Image data cannot be decoded |
| `IMAGE_TOO_LARGE` | Image over 50 megapixels (width × height) |
| `STYLE_NOT_APPLICABLE` | Style property set on a node type it does not apply to |
| `INVALID_TABLE` | Inconsistent table structure (spans, columns, header rows) |
| `INVALID_NODE` | Node type not allowed here (e.g. `pageBreak` in a header) |
| `UNKNOWN_ANCHOR` | A `#id` link names no node `id` in the document |

Each error is `{ "code", "path", "message" }`, with `path` like
`pages[0].children[3].runs[1].font`.
"""


def type_of(p):
    if "$ref" in p:
        name = p["$ref"].split("/")[-1]
        return f"[`{name}`](#{name.lower()})"
    if "const" in p:
        return f"`{json.dumps(p['const'])}`"
    if "enum" in p:
        return " \\| ".join(f"`{json.dumps(v)}`" for v in p["enum"])
    if "oneOf" in p:
        return " or ".join(type_of(o) for o in p["oneOf"])
    t = p.get("type", "")
    if t == "array" and "prefixItems" in p:
        return "`[` " + ", ".join(type_of(i) for i in p["prefixItems"]) + " `]`"
    if t == "object" and "properties" in p:
        keys = ", ".join(f"`{k}`" for k in p["properties"])
        if p.get("required"):
            inner = p["properties"][p["required"][0]]
            return f"`{{{p['required'][0]}: …}}` ({type_of(inner)})"
        return f"object with {keys}"
    if t == "array":
        return f"array of {type_of(p.get('items', {}))}"
    out = f"`{t}`" if t else ""
    if "pattern" in p:
        out += f" matching `{p['pattern']}`"
    return out


def number(v):
    """A bound as written in the schema: `0.000001`, not `1e-06`."""
    return f"{v:f}".rstrip("0").rstrip(".") if isinstance(v, float) else str(v)


def constraints(p):
    parts = []
    for key, label in [
        ("minimum", "≥"),
        ("exclusiveMinimum", ">"),
        ("maximum", "≤"),
        ("minItems", "min items"),
        ("minLength", "min length"),
        ("maxLength", "max length"),
    ]:
        if key in p:
            parts.append(f"{label} {number(p[key])}")
    if "const" in p.get("not", {}):
        parts.append(f"≠ {json.dumps(p['not']['const'])}")
    if "default" in p:
        parts.append(f"default `{json.dumps(p['default'])}`")
    return ", ".join(parts)


def obj_section(name, d):
    lines = [f"### {name}", ""]
    if "description" in d:
        lines += [d["description"], ""]
    props = d.get("properties", {})
    if props:
        req = set(d.get("required", []))
        lines += ["| Property | Type | Required | Notes |", "|---|---|---|---|"]
        for k, p in props.items():
            notes = "; ".join(x for x in [constraints(p), p.get("description", "").replace("\n", " ")] if x)
            lines.append(f"| `{k}` | {type_of(p)} | {'yes' if k in req else ''} | {notes} |")
        lines.append("")
    elif "oneOf" in d or "type" in d or "enum" in d:
        lines += [f"Type: {type_of(d)}" + (f" ({constraints(d)})" if constraints(d) else ""), ""]
    return lines


out = [INTRO, "## Document (top level)", ""]
out += obj_section("Document", schema)[2:]
out += ["## Definitions", ""]
for name, d in defs.items():
    out += obj_section(name, d)
text = "\n".join(out).rstrip() + "\n"

target = root / "docs/SCHEMA.md"
if "--check" in sys.argv:
    if not target.exists() or target.read_text() != text:
        print("docs/SCHEMA.md is stale: run scripts/gen-schema-doc.py")
        sys.exit(1)
    print("docs/SCHEMA.md: up to date")
else:
    target.write_text(text)
    print(f"wrote {target.relative_to(root)}")
