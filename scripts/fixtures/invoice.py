#!/usr/bin/env python3
"""Generates fixtures/docs/invoice.json (a realistic one-page invoice)."""
import json
import pathlib

INK = [0.09, 0.1, 0.13, 1]
MUTED = [0.42, 0.45, 0.52, 1]
LINE = [0.88, 0.89, 0.92, 1]
ACCENT = [0.16, 0.36, 0.86, 1]
WASH = [0.96, 0.97, 0.99, 1]


def run(text, font="Inter-Regular", size=9, color=INK, **kw):
    return {"text": text, "font": font, "size": size, "color": color, **kw}


def text(*runs, **style):
    node = {"type": "text", "runs": list(runs)}
    if style:
        node["style"] = style
    return node


def box(*children, **style):
    node = {"type": "box", "style": style}
    if children:
        node["children"] = list(children)
    return node


def edges(v):
    return {"top": v, "right": v, "bottom": v, "left": v}


ITEMS = [
    ("Engine licence — annual", "Inkoxide PDF engine, production use, 1 server", 1, 1200.00),
    ("Integration support", "Remote pairing sessions (hours)", 6, 95.00),
    ("Custom font onboarding", "Validation and embedding of 4 corporate typefaces", 4, 45.00),
    ("Priority SLA", "Next-business-day response, 12 months", 1, 480.00),
    ("Training workshop", "Half-day workshop for the document team", 1, 650.00),
]

COLS = [("Description", None), ("Qty", 40), ("Unit price", 70), ("Amount", 80)]


def cell(content, width, align="left", bold=False, color=INK):
    style = {"flexGrow": 1, "flexBasis": 0} if width is None else {"width": width}
    return box(
        text(run(content, "Inter-Bold" if bold else "Inter-Regular", 9, color), textAlign=align),
        **style,
    )


def money(v):
    return f"€{v:,.2f}"


header_row = box(
    *[cell(name.upper(), w, "left" if i == 0 else "right", True, MUTED) for i, (name, w) in enumerate(COLS)],
    flexDirection="row", gap=10, padding={"top": 7, "bottom": 7, "left": 10, "right": 10},
    backgroundColor=WASH, borderWidth={"bottom": 1}, borderColor={"bottom": LINE},
)

rows = []
for title, desc, qty, price in ITEMS:
    rows.append(box(
        box(
            text(run(title, "Inter-Bold", 9.5)),
            text(run(desc, size=8.5, color=MUTED), margin={"top": 2}),
            flexGrow=1, flexBasis=0,
        ),
        cell(str(qty), 40, "right"),
        cell(money(price), 70, "right"),
        cell(money(qty * price), 80, "right", True),
        flexDirection="row", gap=10, padding={"top": 8, "bottom": 8, "left": 10, "right": 10},
        borderWidth={"bottom": 1}, borderColor={"bottom": LINE},
    ))

subtotal = sum(q * p for _, _, q, p in ITEMS)
vat = round(subtotal * 0.19, 2)


def total_line(label, value, strong=False):
    return box(
        text(run(label, "Inter-Bold" if strong else "Inter-Regular", 10 if strong else 9, INK if strong else MUTED)),
        text(run(value, "Inter-Bold" if strong else "Inter-Regular", 10 if strong else 9)),
        flexDirection="row", justifyContent="space-between",
    )


def party(label, lines):
    return box(
        text(run(label.upper(), "Inter-Bold", 7.5, MUTED, letterSpacing=0.6), margin={"bottom": 4}),
        text(run(lines[0], "Inter-Bold", 10)),
        *[text(run(l, size=9, color=MUTED), margin={"top": 1}) for l in lines[1:]],
        flexGrow=1, flexBasis=0,
    )


doc = {
    "version": 1,
    "meta": {"title": "Invoice INV-2026-0142", "author": "Inkoxide Ltd", "lang": "en"},
    "fonts": [
        {"id": "Inter-Regular", "family": "Inter"},
        {"id": "Inter-Bold", "family": "Inter", "weight": 700},
    ],
    "pages": [{
        "size": {"width": 595.28, "height": 841.89},
        "margin": edges(48),
        "children": [
            box(
                box(
                    box(width=26, height=26, backgroundColor=ACCENT, borderRadius=6),
                    text(run("Inkoxide", "Inter-Bold", 16)),
                    flexDirection="row", alignItems="center", gap=8,
                ),
                box(
                    text(run("INVOICE", "Inter-Bold", 20, ACCENT, letterSpacing=1.5), textAlign="right"),
                    text(run("INV-2026-0142", size=10, color=MUTED), textAlign="right"),
                    alignItems="flex-end",
                ),
                flexDirection="row", justifyContent="space-between", alignItems="flex-start",
                margin={"bottom": 28},
            ),
            box(
                party("Billed to", ["Studio North Ltd", "11 Harbour Street", "Dublin D02 X285, Ireland", "Company No. 512345"]),
                party("From", ["Inkoxide Ltd", "25 Riverside Avenue", "Cork T12 K8AF, Ireland", "Company No. 654321"]),
                box(
                    text(run("ISSUED", "Inter-Bold", 7.5, MUTED, letterSpacing=0.6), margin={"bottom": 4}),
                    text(run("8 October 2026", size=9)),
                    text(run("DUE", "Inter-Bold", 7.5, MUTED, letterSpacing=0.6), margin={"top": 8, "bottom": 4}),
                    text(run("7 November 2026", size=9)),
                    width=110,
                ),
                flexDirection="row", gap=20, margin={"bottom": 28},
            ),
            box(header_row, *rows,
                borderWidth=edges(1), borderColor=edges(LINE), borderRadius=6, overflow="hidden"),
            box(
                box(
                    total_line("Subtotal", money(subtotal)),
                    total_line("VAT 19%", money(vat)),
                    box(height=1, backgroundColor=LINE, margin={"top": 2, "bottom": 2}),
                    total_line("Total due", money(subtotal + vat), True),
                    width=210, gap=6, padding=edges(12), backgroundColor=WASH, borderRadius=6,
                ),
                flexDirection="row", justifyContent="flex-end", margin={"top": 16},
            ),
            box(flexGrow=1),
            box(
                text(run("Payment details", "Inter-Bold", 9), margin={"bottom": 3}),
                text(run("IBAN IE64 AAAA 9311 5212 3456 78 · BIC AAAAIE2D · Reference INV-2026-0142", size=8.5, color=MUTED)),
                text(run("Thank you for your business. Late payments accrue interest of 0.02% per day as per the agreement.",
                         size=8.5, color=MUTED), margin={"top": 6}, lineHeight=1.4),
                borderWidth={"top": 1}, borderColor={"top": LINE}, padding={"top": 12},
            ),
        ],
    }],
}

out = pathlib.Path(__file__).resolve().parents[2] / "fixtures/docs/invoice.json"
out.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n")
print(f"wrote {out}")
