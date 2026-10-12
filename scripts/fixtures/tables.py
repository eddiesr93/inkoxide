#!/usr/bin/env python3
"""Generates the table fixtures: table-basic, table-split-header-repeat.
The 500-page table dump for benchmarks is generated on demand by
scripts/fixtures/table_dump.py (too large to commit)."""
from common import A4, ACCENT, FONTS, INK, edges, run, text, write

MUTED = [0.45, 0.47, 0.53, 1]
LINE = [0.82, 0.84, 0.88, 1]
HEAD = [0.93, 0.95, 0.99, 1]
ZEBRA = [0.975, 0.98, 0.99, 1]


def label(t, font="Inter-Regular", size=9, color=INK, **style):
    """A text node of one run."""
    return text(run(t, font, size, color), **style)


def cell(content, align="left", bold=False, color=INK, **style):
    st = {"padding": {"top": 5, "right": 7, "bottom": 5, "left": 7}}
    st.update(style)
    return {"style": st, "children": [label(content, "Inter-Bold" if bold else "Inter-Regular", 9, color, textAlign=align)]}


def grid_style(w=0.75, c=LINE):
    return {"borderWidth": edges(w), "borderColor": edges(c)}


# --- table-basic -------------------------------------------------------
g = grid_style()
basic_rows = [
    {"backgroundColor": HEAD, "cells": [cell("Fixed 90pt", bold=True, **g), cell("1fr", bold=True, **g),
                                         cell("2fr", bold=True, **g), cell("auto", bold=True, **g)]},
    {"cells": [cell("Row span 2, middle", verticalAlign="middle", **g) | {"rowSpan": 2},
               cell("Column span 2 across the fr columns", **g) | {"colSpan": 2},
               cell("short", **g)]},
    {"backgroundColor": ZEBRA, "cells": [cell("A wrapping paragraph inside a fractional column keeps the row tall.", **g),
                                         cell("bottom", verticalAlign="bottom", **g), cell("auto grows to fit", **g)]},
    {"cells": [cell("plain", **g), cell("right", align="right", **g), cell("center", align="center", **g),
               cell("1 234,56", align="right", **g)]},
    {"minHeight": 40, "backgroundColor": ZEBRA, "cells": [cell("minHeight 40", **g), cell("", **g),
               cell("Nested box", **g) | {"children": [
                   {"type": "box", "style": {"flexDirection": "row", "gap": 4, "alignItems": "center"}, "children": [
                       {"type": "box", "style": {"width": 10, "height": 10, "borderRadius": 5, "backgroundColor": ACCENT}},
                       label("box + text in a cell")]}]},
               cell("—", align="center", **g)]},
]
thick = {"borderWidth": {"bottom": 2}, "borderColor": {"bottom": INK}}
minimal_rows = [{"cells": [cell("Item", bold=True, **thick), cell("Qty", "right", True, **thick), cell("Price", "right", True, **thick)]}]
for i, (name, q, p) in enumerate([("Paper", 3, "4.50"), ("Ink", 1, "12.00"), ("Binding", 2, "7.25")]):
    b = {"borderWidth": {"bottom": 0.5}, "borderColor": {"bottom": LINE}}
    minimal_rows.append({"cells": [cell(name, **b), cell(str(q), "right", **b), cell(p, "right", **b)]})

write("table-basic", {
    "version": 1, "meta": {"title": "Table basics", "lang": "en"}, "fonts": FONTS,
    "pages": [{"size": A4, "margin": edges(48), "children": [
        label("Tables", "Inter-Bold", 20, margin={"bottom": 4}),
        label("Fixed, fr and auto columns · row and column spans · collapsed borders · vertical alignment",
             size=10, color=MUTED, margin={"bottom": 14}),
        {"type": "table", "columns": [{"width": 90}, {"width": "1fr"}, {"width": "2fr"}, {"width": "auto"}],
         "headerRows": 1, "rows": basic_rows},
        label("Rules only (thick header line, hairlines between rows)", "Inter-Bold", 11, margin={"top": 20, "bottom": 6}),
        {"type": "table", "style": {"width": 300}, "columns": [{"width": "1fr"}, {"width": 50}, {"width": 70}],
         "headerRows": 1, "rows": minimal_rows},
        label("Outer border on the table box, rounded", "Inter-Bold", 11, margin={"top": 20, "bottom": 6}),
        {"type": "table", "style": {"borderWidth": edges(1), "borderColor": edges(ACCENT), "borderRadius": 6,
                                     "overflow": "hidden"},
         "columns": [{"width": "1fr"}, {"width": "1fr"}], "headerRows": 1,
         "rows": [{"backgroundColor": HEAD, "cells": [cell("Key", bold=True), cell("Value", bold=True)]}] +
                 [{"cells": [cell(f"key-{i}"), cell(f"value {i}")], **({"backgroundColor": ZEBRA} if i % 2 else {})} for i in range(4)]},
    ]}],
})

# --- table-split-header-repeat -------------------------------------------
head = {"backgroundColor": ACCENT, "cells": [
    {"style": {"padding": edges(6)}, "children": [label(h, "Inter-Bold", 9, [1, 1, 1, 1], textAlign=a)]}
    for h, a in [("#", "right"), ("Description", "left"), ("Category", "left"), ("Amount", "right")]]}
rows = [head]
cats = ["Hardware", "Services", "Licences", "Travel", "Training"]
for i in range(1, 121):
    b = {"borderWidth": {"bottom": 0.5}, "borderColor": {"bottom": LINE}}
    desc = f"Line item {i}" + (" with a longer description that wraps onto a second line in this column" if i % 7 == 0 else "")
    row = {"cells": [cell(str(i), "right", **b), cell(desc, **b), cell(cats[i % 5], color=MUTED, **b),
                     cell(f"{(i * 37.5) % 900 + 12:.2f}", "right", **b)]}
    if i % 2 == 0:
        row["backgroundColor"] = ZEBRA
    rows.append(row)
write("table-split-header-repeat", {
    "version": 1, "meta": {"title": "Table split with header repeat", "lang": "en"}, "fonts": FONTS,
    "pages": [{"size": A4, "margin": edges(48),
               "footer": {"type": "box", "style": {"padding": {"bottom": 20, "right": 48}, "alignItems": "flex-end"},
                          "children": [{"type": "text", "runs": [run("Page ", size=8, color=MUTED),
                                        {"placeholder": "pageNumber", "font": "Inter-Bold", "size": 8, "color": INK},
                                        run(" / ", size=8, color=MUTED),
                                        {"placeholder": "totalPages", "font": "Inter-Bold", "size": 8, "color": INK}]}]},
               "children": [
                   label("Ledger", "Inter-Bold", 20, margin={"bottom": 4}),
                   label("120 rows; the blue header row repeats at the top of every page.", size=10, color=MUTED, margin={"bottom": 12}),
                   {"type": "table", "columns": [{"width": 30}, {"width": "1fr"}, {"width": 80}, {"width": 70}],
                    "headerRows": 1, "rows": rows},
                   label("End of ledger.", "Inter-Bold", 11, margin={"top": 10}),
               ]}],
})
