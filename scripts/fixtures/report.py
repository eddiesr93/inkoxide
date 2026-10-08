#!/usr/bin/env python3
"""Generates the ~100-page report (paragraphs + 40 tables, 2 fonts) used
for benchmarks. Writes target/generated/report-100p.json.

  scripts/fixtures/report.py [sections]
"""
import json
import pathlib
import random
import sys

SECTIONS = int(sys.argv[1]) if len(sys.argv) > 1 else 40
OUT = pathlib.Path(__file__).resolve().parents[2] / "target/generated"
OUT.mkdir(parents=True, exist_ok=True)
rng = random.Random(42)
INK = [0.1, 0.11, 0.14, 1]
MUTED = [0.4, 0.42, 0.48, 1]
LINE = [0.85, 0.86, 0.9, 1]
WORDS = ("analysis revenue quarterly performance growth market segment customer retention "
         "operational efficiency forecast budget allocation strategic initiative regional "
         "distribution supply chain logistics procurement compliance governance framework "
         "implementation measurable outcomes stakeholder engagement sustainability digital "
         "transformation infrastructure investment portfolio diversification risk mitigation").split()


def sentence():
    w = [rng.choice(WORDS) for _ in range(rng.randint(8, 18))]
    return " ".join(w).capitalize() + "."


def para(n=None):
    return {"type": "text", "style": {"textAlign": "justify", "lineHeight": 1.45, "hyphenate": True,
                                      "margin": {"bottom": 8}},
            "runs": [{"text": " ".join(sentence() for _ in range(n or rng.randint(3, 7))),
                      "font": "Inter-Regular", "size": 10, "color": INK}]}


def heading(t):
    return {"type": "text", "break": {"minPresenceAhead": 60},
            "style": {"margin": {"top": 10, "bottom": 6}},
            "runs": [{"text": t, "font": "Inter-Bold", "size": 15, "color": INK}]}


def cell(t, align="left", font="Inter-Regular"):
    return {"style": {"padding": {"top": 3, "right": 5, "bottom": 3, "left": 5},
                      "borderWidth": {"bottom": 0.5}, "borderColor": {"bottom": LINE}},
            "children": [{"type": "text", "style": {"textAlign": align},
                          "runs": [{"text": t, "font": font, "size": 8.5, "color": INK}]}]}


def table(k):
    head = {"backgroundColor": [0.93, 0.95, 0.99, 1],
            "cells": [cell(h, a, "Inter-Bold") for h, a in
                      [("Code", "left"), ("Description", "left"), ("Units", "right"), ("Price", "right"), ("Amount", "right")]]}
    rows = [head]
    for i in range(rng.randint(20, 40)):
        u = rng.randint(1, 500)
        p = rng.uniform(1, 900)
        rows.append({"cells": [cell(f"S{k:02d}-{i:03d}"), cell(" ".join(rng.choice(WORDS) for _ in range(rng.randint(2, 6)))),
                               cell(str(u), "right"), cell(f"{p:,.2f}", "right"), cell(f"{u * p:,.2f}", "right")]})
    return {"type": "table", "headerRows": 1, "style": {"margin": {"top": 4, "bottom": 12}}, "rows": rows,
            "columns": [{"width": 60}, {"width": "1fr"}, {"width": 50}, {"width": 70}, {"width": 80}]}


children = []
for s in range(1, SECTIONS + 1):
    children.append(heading(f"{s}. Section {s}: " + " ".join(rng.choice(WORDS) for _ in range(3)).title()))
    for _ in range(rng.randint(7, 12)):
        children.append(para())
    children.append(table(s))
    children.append(para(2))

footer = {"type": "box", "style": {"flexDirection": "row", "justifyContent": "space-between",
                                   "padding": {"top": 12, "right": 48, "bottom": 20, "left": 48}},
          "children": [
              {"type": "text", "runs": [{"text": "Quarterly report", "font": "Inter-Regular", "size": 8, "color": MUTED}]},
              {"type": "text", "runs": [
                  {"text": "Page ", "font": "Inter-Regular", "size": 8, "color": MUTED},
                  {"placeholder": "pageNumber", "font": "Inter-Regular", "size": 8, "color": MUTED},
                  {"text": " of ", "font": "Inter-Regular", "size": 8, "color": MUTED},
                  {"placeholder": "totalPages", "font": "Inter-Regular", "size": 8, "color": MUTED}]}]}
doc = {"version": 1, "meta": {"title": "Report", "lang": "en"},
       "fonts": [{"id": "Inter-Regular", "family": "Inter"}, {"id": "Inter-Bold", "family": "Inter", "weight": 700}],
       "pages": [{"size": {"width": 595.28, "height": 841.89},
                  "margin": {"top": 48, "right": 48, "bottom": 56, "left": 48},
                  "footer": footer, "children": children}]}
path = OUT / "report-100p.json"
path.write_text(json.dumps(doc, separators=(",", ":")))
print(f"wrote {path} ({path.stat().st_size // 1024} KiB)")
