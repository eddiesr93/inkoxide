"""Shared builders of the fixture generators in this directory: nodes, the
colours and page size most fixtures use, and the writer of
fixtures/docs/<name>.json. A generator keeps its own colours where its look
differs; the committed fixtures and their reference images depend on these
values, so changing one means regenerating both."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
DOCS = ROOT / "fixtures/docs"

R, B = "Inter-Regular", "Inter-Bold"
FONTS = [{"id": R, "family": "Inter"}, {"id": B, "family": "Inter", "weight": 700}]
A4 = {"width": 595.28, "height": 841.89}
INK = [0.1, 0.11, 0.14, 1]
ACCENT = [0.16, 0.36, 0.86, 1]


def edges(v):
    return {"top": v, "right": v, "bottom": v, "left": v}


def run(text, font=R, size=9, color=INK, **kw):
    return {"text": text, "font": font, "size": size, "color": color, **kw}


def text(*runs, brk=None, **style):
    n = {"type": "text", "runs": list(runs)}
    if style:
        n["style"] = style
    if brk:
        n["break"] = brk
    return n


def box(*children, brk=None, **style):
    n = {"type": "box", "style": style}
    if children:
        n["children"] = list(children)
    if brk:
        n["break"] = brk
    return n


def write(name, doc, compact=False):
    """Writes fixtures/docs/<name>.json: indented, or compact for large documents."""
    path = DOCS / f"{name}.json"
    if compact:
        path.write_text(json.dumps(doc, separators=(",", ":"), ensure_ascii=False) + "\n", encoding="utf-8")
    else:
        path.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {path.relative_to(ROOT)} ({path.stat().st_size // 1024} KiB)")
