#!/usr/bin/env python3
"""Generates the 20k-row table dump (~500 pages) used for benchmarks.
Too large to commit: writes to target/generated/table-dump-500p.json.

  scripts/fixtures/table_dump.py [rows]
"""
import json
import sys

from common import A4, FONTS, ROOT, edges, run

ROWS = int(sys.argv[1]) if len(sys.argv) > 1 else 20000
OUT = ROOT / "target/generated"
OUT.mkdir(parents=True, exist_ok=True)
LINE = [0.85, 0.86, 0.9, 1]


def cell(t, align="left", font="Inter-Regular"):
    return {"style": {"padding": {"top": 3, "right": 5, "bottom": 3, "left": 5},
                      "borderWidth": {"bottom": 0.5}, "borderColor": {"bottom": LINE}},
            "children": [{"type": "text", "style": {"textAlign": align},
                          "runs": [run(t, font, 8)]}]}


head = {"backgroundColor": [0.93, 0.95, 0.99, 1],
        "cells": [cell(h, a, "Inter-Bold") for h, a in [("ID", "right"), ("Name", "left"), ("Region", "left"),
                                                         ("Quantity", "right"), ("Total", "right")]]}
regions = ["North", "South", "East", "West", "Central"]
rows = [head] + [{"cells": [cell(str(i), "right"), cell(f"Customer {i:05d}"), cell(regions[i % 5]),
                            cell(str(i * 7 % 1000), "right"), cell(f"{i * 13.37 % 10000:.2f}", "right")]}
                 for i in range(1, ROWS + 1)]
doc = {"version": 1, "meta": {"title": "Table dump"},
       "fonts": FONTS,
       "pages": [{"size": A4, "margin": edges(36),
                  "children": [{"type": "table", "headerRows": 1, "rows": rows,
                                "columns": [{"width": 50}, {"width": "2fr"}, {"width": "1fr"}, {"width": 60}, {"width": 70}]}]}]}
path = OUT / "table-dump-500p.json"
path.write_text(json.dumps(doc, separators=(",", ":")))
print(f"wrote {path} ({path.stat().st_size // 1024} KiB, {ROWS} rows)")
