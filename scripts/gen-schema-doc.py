#!/usr/bin/env python3
"""Generates docs/SCHEMA.md from packages/schema/document.v1.json, and
checks that its table of validation codes lists exactly the codes of
`ValidationCode` (crates/core/src/error.rs).

  scripts/gen-schema-doc.py          # rewrite docs/SCHEMA.md
  scripts/gen-schema-doc.py --check  # fail if docs/SCHEMA.md is stale
"""
import json
import pathlib
import re
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
  padding and horizontal properties, as in CSS). Gaps resolve against the
  box's own content box, and a percentage height needs a containing block
  with a set height: the page content box, for a child of the page.
- **Numbers.** An integer may be written with a zero fraction (`400.0`),
  as JSON Schema reads it. Other numbers are read as 32-bit floats, so a
  number past a bound by less than a float's precision there is read as
  the bound and accepted (`1000000.01` where the maximum is `1000000`).
  Percentages and `fr` sizes are bounded as written: `"10000.0001%"` is
  out of range.
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
  is filled in after pagination, and the text wraps and aligns at the
  width of the digits. A header, a footer or an overlay is laid out with
  each page's numbers. Flow text is laid out with the numbers of the page
  it lands on, and the flow is paginated again if they move it. Flow text
  painted on several pages (in a fixed node or a table's header rows)
  reserves the width of `999` in the run's font for each page's digits; a
  longer number can overflow that space.
- **Images.** Intrinsic size is the pixel size at 96 dpi (1 px = 0.75 pt),
  like CSS. An image with no `width` and no `height` that is wider than
  the room it has is scaled down to fit it, keeping its aspect ratio
  (CSS would let it overflow). `fit` defaults to `fill`, like CSS `object-fit`, and
  `objectPosition` places a fitted image in its box like `object-position`
  (centred by default; what falls outside the box is clipped).

## Validation error codes

| Code | Meaning |
|---|---|
| `PARSE_ERROR` | Input is not well-formed JSON or MessagePack |
| `UNSUPPORTED_VERSION` | `version` is not `1` |
| `UNKNOWN_FIELD` | Property not allowed at this position |
| `MISSING_FIELD` | Required property missing |
| `INVALID_TYPE` | Wrong JSON type |
| `INVALID_VALUE` | Value not in the allowed set / wrong format; a string too short or too long; an array with more items than its fixed length; `break` where pagination does not apply (headers, footers, table cells, fixed overlays), or a forced break where it would do nothing (see `Break.before`) |
| `OUT_OF_RANGE` | Number outside its allowed range; page margins that leave no room for content |
| `DUPLICATE_ID` | Font id declared twice, or form field `name` used twice (except by the buttons of one radio group), or used both for a field and for a group of fields (`a` and `a.b`) |
| `UNKNOWN_FONT` | Font id not declared in `fonts` |
| `FONT_NOT_REGISTERED` | Declared font has no registered bytes |
| `UNKNOWN_IMAGE` | Image `src` not provided in the render options |
| `INVALID_IMAGE` | Image data cannot be decoded (a JPEG is not decoded: its structure is checked) |
| `IMAGE_TOO_LARGE` | Raster image (not SVG) over 50 megapixels (width × height). An SVG has its own limit: a width or height over 100,000 px is `INVALID_IMAGE` |
| `STYLE_NOT_APPLICABLE` | Style property set on a node type it does not apply to |
| `INVALID_TABLE` | Inconsistent table structure (spans, columns, header rows) |
| `INVALID_NODE` | Node type not allowed here (e.g. `pageBreak` in a header) |
| `UNKNOWN_ANCHOR` | A `#id` link names no displayed node `id` in the page body, flow or overlays (ids in headers, footers and under `display: none` are not destinations) |
| `PDFA_CONFLICT` | The document uses something the PDF/A level does not allow: at any level, a page side under 3 points (at the page's `size`); under PDF/A-1b, transparency (a colour with alpha below 1, `opacity` below 1, `debug`, an image, inline image, emoji image or background image with an alpha channel, an SVG drawn with opacity, a mask, a blend mode, an isolated group or a clip path other than one path, or a colour glyph that needs transparency: a bitmap with an alpha channel or a layer with an opacity). Only a render reports it, at the first such use |

Each error is `{ "code", "path", "message" }`, with `path` like
`pages[0].children[3].runs[1].font`. Validation returns at most 100
errors. A problem found while reading the document's structure (input
that does not parse, an unknown or missing property, a wrong type, a
value the reader rejects) is returned alone, as the rest cannot be
checked.
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
        return " or ".join(nested(o) for o in p["oneOf"])
    t = p.get("type", "")
    if t == "array" and "prefixItems" in p:
        return "`[` " + ", ".join(nested(i) for i in p["prefixItems"]) + " `]`"
    if t == "object" and "properties" in p:
        if p.get("required"):
            # Every required key, with its type: `{min, max}` (`min`: …; `max`: …).
            req = p["required"]
            types = "; ".join(f"`{k}`: {nested(p['properties'][k])}" for k in req)
            return f"`{{{', '.join(req)}}}` ({types})"
        keys = ", ".join(f"`{k}`" for k in p["properties"])
        return f"object with {keys}"
    if t == "array":
        items = p.get("items", {})
        each = constraints(items, defaults=False)
        return f"array of {type_of(items)}" + (f" (each {each})" if each else "")
    out = f"`{t}`" if t else ""
    if "pattern" in p:
        out += f" matching `{p['pattern']}`"
    return out


def nested(p):
    """The type of a value inside another type (a `oneOf` branch, an object
    key, an array item), with its constraints, which have no Notes column."""
    c = constraints(p, defaults=False)
    return type_of(p) + (f" ({c})" if c else "")


def number(v):
    """A bound as written in the schema: `0.000001`, not `1e-06`."""
    return f"{v:f}".rstrip("0").rstrip(".") if isinstance(v, float) else str(v)


def constraints(p, defaults=True):
    parts = []
    for key, label in [
        ("minimum", "≥"),
        ("exclusiveMinimum", ">"),
        ("maximum", "≤"),
        ("exclusiveMaximum", "<"),
        ("minLength", "min length"),
        ("maxLength", "max length"),
        ("minProperties", "min properties"),
        ("maxProperties", "max properties"),
    ]:
        if key in p:
            parts.append(f"{label} {number(p[key])}")
    if "minItems" in p and p.get("maxItems") == p["minItems"]:
        parts.append(f"exactly {p['minItems']} items")
    else:
        for key, label in [("minItems", "min items"), ("maxItems", "max items")]:
            if key in p:
                parts.append(f"{label} {p[key]}")
    if "format" in p:
        parts.append(f"format `{p['format']}`")
    if "const" in p.get("not", {}):
        parts.append(f"≠ {json.dumps(p['not']['const'])}")
    if "pattern" in p.get("not", {}):
        parts.append(f"not matching `{p['not']['pattern']}`")
    if "default" in p and defaults:
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


# The code table must list exactly the codes of `ValidationCode`.
error_rs = (root / "crates/core/src/error.rs").read_text()
engine_codes = re.findall(r'^\s*\w+ = "([A-Z_]+)",$', error_rs.split("validation_codes! {")[1].split("\n}")[0], re.M)
table_codes = re.findall(r"^\| `([A-Z_]+)` \|", INTRO, re.M)
if sorted(engine_codes) != sorted(table_codes):
    print(
        "the validation code table in scripts/gen-schema-doc.py differs from ValidationCode "
        f"in crates/core/src/error.rs: missing {sorted(set(engine_codes) - set(table_codes))}, "
        f"extra {sorted(set(table_codes) - set(engine_codes))}"
    )
    sys.exit(1)

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
