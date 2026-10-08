#!/usr/bin/env python3
"""Generates fixtures/docs/report-100p-realistic.json: a ~100-page
business report with paragraphs (some with bold and linked phrases),
40 tables of four kinds (line items with a spanning total row, striped
KPIs, a wide numeric grid, key/value pairs), two fonts, and a header and
footer with page numbers. It is the input of scripts/bench-headline.sh.

The output is committed; scripts/ci.sh regenerates it and fails when it
differs, so edit this script, not the JSON.

  scripts/fixtures/report_realistic.py
"""
import json
import pathlib
import random

OUT = pathlib.Path(__file__).resolve().parents[2] / "fixtures/docs/report-100p-realistic.json"
rng = random.Random(2026)
R, B = "Inter-Regular", "Inter-Bold"
INK = [0.1, 0.11, 0.14, 1]
MUTED = [0.4, 0.42, 0.48, 1]
ACCENT = [0.12, 0.29, 0.6, 1]
LINE = [0.85, 0.86, 0.9, 1]
HEAD_BG = [0.93, 0.95, 0.99, 1]
STRIPE = [0.97, 0.975, 0.985, 1]
WORDS = ("analysis revenue quarterly performance growth market segment customer retention "
         "operational efficiency forecast budget allocation strategic initiative regional "
         "distribution supply chain logistics procurement compliance governance framework "
         "implementation measurable outcomes stakeholder engagement sustainability digital "
         "transformation infrastructure investment portfolio diversification risk mitigation "
         "headcount margin pricing contract renewal pipeline conversion inventory turnover").split()
REGIONS = ["North", "South", "East", "West", "Central", "Export"]
MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]


def words(lo, hi):
    return " ".join(rng.choice(WORDS) for _ in range(rng.randint(lo, hi)))


def sentence():
    return words(8, 18).capitalize() + "."


def run(text, font=R, size=10, color=INK, **kw):
    r = {"text": text, "font": font, "size": size, "color": color}
    r.update(kw)
    return r


def para(n=None):
    runs = []
    for _ in range(n or rng.randint(3, 7)):
        roll = rng.random()
        if roll < 0.15:
            runs.append(run(words(1, 3).capitalize() + ": ", B))
            runs.append(run(sentence() + " "))
        elif roll < 0.2:
            runs.append(run(words(2, 4).capitalize(), color=ACCENT, decoration="underline",
                            link="https://example.com/" + rng.choice(WORDS)))
            runs.append(run(" " + words(6, 12) + ". "))
        else:
            runs.append(run(sentence() + " "))
    runs[-1]["text"] = runs[-1]["text"].rstrip()
    return {"type": "text", "style": {"textAlign": "justify", "lineHeight": 1.45, "hyphenate": True,
                                      "margin": {"bottom": 8}}, "runs": runs}


def heading(n, t):
    return {"type": "text", "id": f"section-{n}", "bookmark": {"title": t},
            "break": {"minPresenceAhead": 80},
            "style": {"margin": {"top": 12, "bottom": 6}}, "runs": [run(t, B, 15)]}


def caption(t):
    return {"type": "text", "style": {"margin": {"top": 2, "bottom": 4}}, "runs": [run(t, B, 9, MUTED)]}


def cell(t, align="left", font=R, size=8.5, **extra):
    c = {"style": {"padding": {"top": 3, "right": 5, "bottom": 3, "left": 5},
                   "borderWidth": {"bottom": 0.5}, "borderColor": {"bottom": LINE}},
         "children": [{"type": "text", "style": {"textAlign": align},
                       "runs": [run(t, font, size)]}]}
    c.update(extra)
    return c


def money(v):
    return f"{v:,.2f}"


def line_items(k):
    cols = [("Code", "left"), ("Description", "left"), ("Units", "right"), ("Price", "right"), ("Amount", "right")]
    rows = [{"backgroundColor": HEAD_BG, "cells": [cell(h, a, B) for h, a in cols]}]
    total = 0.0
    for i in range(rng.randint(15, 35)):
        u = rng.randint(1, 500)
        p = rng.uniform(1, 900)
        total += u * p
        rows.append({"cells": [cell(f"S{k:02d}-{i:03d}"), cell(words(2, 6)), cell(str(u), "right"),
                               cell(money(p), "right"), cell(money(u * p), "right")]})
    rows.append({"cells": [cell("Total", "right", B, colSpan=4), cell(money(total), "right", B)]})
    return {"type": "table", "headerRows": 1, "style": {"margin": {"bottom": 12}}, "rows": rows,
            "columns": [{"width": 60}, {"width": "1fr"}, {"width": 50}, {"width": 70}, {"width": 80}]}


def kpis():
    rows = [{"backgroundColor": HEAD_BG,
             "cells": [cell(h, a, B) for h, a in [("Indicator", "left"), ("Target", "right"),
                                                   ("Actual", "right"), ("Status", "center")]]}]
    for i in range(rng.randint(6, 12)):
        target = rng.uniform(50, 100)
        actual = target * rng.uniform(0.8, 1.2)
        ok = actual >= target
        rows.append({"backgroundColor": STRIPE if i % 2 else [1, 1, 1, 1],
                     "cells": [cell(words(2, 4).capitalize()), cell(f"{target:.1f}%", "right"),
                               cell(f"{actual:.1f}%", "right"),
                               cell("On track" if ok else "Behind", "center", B)]})
    return {"type": "table", "headerRows": 1, "style": {"margin": {"bottom": 12}}, "rows": rows,
            "columns": [{"width": "2fr"}, {"width": "1fr"}, {"width": "1fr"}, {"width": "1fr"}]}


def wide_grid():
    rows = [{"backgroundColor": HEAD_BG,
             "cells": [cell("Region", "left", B, 7.5)] + [cell(m, "right", B, 7.5) for m in MONTHS]
             + [cell("H1", "right", B, 7.5)]}]
    for reg in REGIONS:
        vals = [rng.uniform(100, 9000) for _ in MONTHS]
        rows.append({"cells": [cell(reg, size=7.5)] + [cell(f"{v:,.0f}", "right", size=7.5) for v in vals]
                     + [cell(f"{sum(vals):,.0f}", "right", B, 7.5)]})
    return {"type": "table", "headerRows": 1, "break": {"inside": "avoid"},
            "style": {"margin": {"bottom": 12}}, "rows": rows,
            "columns": [{"width": 60}] + [{"width": "1fr"}] * len(MONTHS) + [{"width": 55}]}


def key_values():
    rows = [{"cells": [cell(words(1, 3).capitalize(), font=B), cell(sentence())]}
            for _ in range(rng.randint(4, 8))]
    return {"type": "table", "style": {"margin": {"bottom": 12}}, "rows": rows,
            "columns": [{"width": 130}, {"width": "1fr"}]}


TABLES = [("Line items", line_items), ("Key indicators", lambda k: kpis()),
          ("Monthly volume by region", lambda k: wide_grid()), ("Notes", lambda k: key_values())]

children = [
    {"type": "text", "runs": [run("Annual Operations Report 2026", B, 24)]},
    {"type": "text", "style": {"margin": {"top": 4, "bottom": 18}},
     "runs": [run("Prepared for the board of directors · confidential", size=11, color=MUTED)]},
]
for s in range(1, 41):
    children.append(heading(s, f"{s}. " + words(2, 4).title()))
    for _ in range(rng.randint(10, 16)):
        children.append(para())
    name, make = TABLES[(s - 1) % len(TABLES)]
    children.append(caption(f"Table {s}. {name}"))
    children.append(make(s))
    children.append(para(2))

header = {"type": "box", "style": {"flexDirection": "row", "justifyContent": "space-between",
                                   "padding": {"top": 22, "right": 48, "bottom": 6, "left": 48}},
          "children": [
              {"type": "text", "runs": [run("Annual Operations Report 2026", B, 8, ACCENT)]},
              {"type": "text", "runs": [run("Confidential", size=8, color=MUTED)]}]}
footer = {"type": "box", "style": {"flexDirection": "row", "justifyContent": "space-between",
                                   "padding": {"top": 12, "right": 48, "bottom": 20, "left": 48}},
          "children": [
              {"type": "text", "runs": [run("Inkoxide Holdings · Operations", size=8, color=MUTED)]},
              {"type": "text", "runs": [
                  run("Page ", size=8, color=MUTED),
                  {"placeholder": "pageNumber", "font": R, "size": 8, "color": MUTED},
                  run(" of ", size=8, color=MUTED),
                  {"placeholder": "totalPages", "font": R, "size": 8, "color": MUTED}]}]}
doc = {"version": 1,
       "meta": {"title": "Annual Operations Report 2026", "author": "Inkoxide Holdings", "lang": "en"},
       "fonts": [{"id": R, "family": "Inter"}, {"id": B, "family": "Inter", "weight": 700}],
       "pages": [{"size": {"width": 595.28, "height": 841.89},
                  "margin": {"top": 52, "right": 48, "bottom": 56, "left": 48},
                  "header": header, "footer": footer, "children": children}]}
OUT.write_text(json.dumps(doc, separators=(",", ":"), ensure_ascii=False) + "\n", encoding="utf-8")
print(f"wrote {OUT.relative_to(OUT.parents[2])} ({OUT.stat().st_size // 1024} KiB)")
