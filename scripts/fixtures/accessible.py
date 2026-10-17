#!/usr/bin/env python3
"""Generates fixtures/docs/accessible.json: a document with every kind of
structure tagged output writes, for the PDF/UA-1 and PDF/A-2a checks of
crates/core/tests/accessibility.rs. A title and a language, headings h1 to
h3, a list, a table with a header row, a row header and spans, figures (an
image with `alt`, a decorative one, an SVG, a `figure` box), a link in a
paragraph and a linked box, a labelled field, a note, a header with the
page number (decoration), a fixed footer (decoration) and a paragraph that
breaks across the two pages."""
import common
from common import A4, ACCENT, INK, R, B, box, edges, run, text, write

LOGO_SVG = (
    "data:image/svg+xml;base64,"
    "PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCI+"
    "PHJlY3Qgd2lkdGg9IjI0IiBoZWlnaHQ9IjI0IiByeD0iNCIgZmlsbD0iIzI5NWJkYiIvPjwvc3ZnPg=="
)


def heading(level, s):
    t = text(run(s, B, 20 - 3 * level), margin={"bottom": 6})
    t["role"] = f"h{level}"
    return t


def para(s, **style):
    return text(run(s, size=10), margin={"bottom": 6}, **style)


def item(label, body):
    li = box(
        box(text(run(label, size=10)), width=14),
        box(text(run(body, size=10)), flexGrow=1),
        flexDirection="row",
    )
    li["role"] = "listItem"
    li["children"][0]["role"] = "label"
    li["children"][1]["role"] = "listBody"
    return li


def cell(s, bold=False, **extra):
    c = {"children": [text(run(s, B if bold else R, 9))], "style": {"padding": edges(4)}}
    c.update(extra)
    return c


LONG = (
    "Tagged output keeps the reading order of the document, whatever the layout does on the page. "
    * 60
)

doc = {
    "version": 1,
    "meta": {"title": "Accessible document", "lang": "en", "author": "Inkoxide"},
    "fonts": common.FONTS,
    "pages": [
        {
            "size": A4,
            "margin": edges(48),
            "header": box(
                text(run("Accessible document — page ", size=8), {"placeholder": "pageNumber", "font": R, "size": 8, "color": INK}),
                padding=edges(16),
            ),
            "children": [
                heading(1, "Accessible document"),
                para("Every part of this page is tagged with what it is."),
                heading(2, "A list"),
                {**box(
                    item("1.", "Headings go down one level at a time."),
                    item("2.", "Images have alternative text."),
                    item("3.", "Fields have labels."),
                    margin={"bottom": 8},
                ), "role": "list"},
                heading(2, "A table"),
                {
                    "type": "table",
                    "columns": [{"width": "2fr"}, {"width": "1fr"}, {"width": "1fr"}],
                    "headerRows": 1,
                    "style": {"margin": {"bottom": 8}},
                    "rows": [
                        {"cells": [cell("Item", True), cell("Qty", True), cell("Price", True)]},
                        {"cells": [cell("Licence", header=True), cell("1"), cell("1,200")]},
                        {"cells": [cell("Support", header=True), cell("6 hours, 95 each", colSpan=2)]},
                        {"cells": [cell("Total", header=True), cell(""), cell("1,770")]},
                    ],
                },
                heading(3, "Figures"),
                {
                    "type": "box",
                    "style": {"flexDirection": "row", "gap": 12, "margin": {"bottom": 8}},
                    "children": [
                        {"type": "image", "data": LOGO_SVG, "alt": "The Inkoxide logo", "style": {"width": 24, "height": 24}},
                        {"type": "image", "data": LOGO_SVG, "alt": "", "style": {"width": 24, "height": 24}},
                        {
                            **box(
                                text(run("Q3", B, 14)),
                                text(run("+12%", size=10, color=ACCENT)),
                                padding=edges(6),
                                backgroundColor=[0.94, 0.95, 0.98, 1],
                            ),
                            "role": "figure",
                            "alt": "Sales rose 12% in the third quarter",
                        },
                    ],
                },
                text(
                    run("Read the ", size=10),
                    run("guide", size=10, color=ACCENT, link="https://example.com/guide"),
                    run(" for more.", size=10),
                    margin={"bottom": 6},
                ),
                {**box(text(run("Jump to the end", size=10, color=ACCENT)), margin={"bottom": 6}), "link": "#end"},
                {"type": "field", "field": {"name": "email", "kind": "text", "font": R, "size": 10, "label": "Your email address"}, "style": {"width": 200, "margin": {"bottom": 6}}},
                {"type": "note", "note": {"contents": "Reviewed by the accessibility team."}},
                {"type": "text", "role": "artifact", "runs": [run("Decoration that is not read.", size=8, color=[0.6, 0.6, 0.6, 1])]},
                para(LONG),
                {**para("The end."), "id": "end"},
                {
                    **text(run("Footer: confidential", size=8)),
                    "break": {"fixed": True},
                    "style": {"position": "absolute", "inset": {"bottom": -30, "left": 0}},
                },
            ],
        }
    ],
}
write("accessible", doc)
