# Schema examples

Three complete documents for schema version 1. Each one validates and
renders as is, once the fonts it declares are registered: the Inter
fonts from `fixtures/fonts/`, under the ids `Inter-Regular` and
`Inter-Bold`. `examples/node-smoke` renders every JSON block in this
file and `scripts/check-schema.py` validates them, so they cannot go
stale.

All lengths are PDF points (1/72 in) and colours are RGBA floats in
[0, 1]. The full reference is [`../docs/SCHEMA.md`](../docs/SCHEMA.md).

## 1. Minimal single page

One A4 page with a 1-inch margin and one line of text. `version`,
`pages` and, for text, the declared font are all that is required.

```json
{
  "version": 1,
  "fonts": [{"id": "Inter-Regular", "family": "Inter"}],
  "pages": [
    {
      "size": {"width": 595.28, "height": 841.89},
      "margin": {"top": 72, "right": 72, "bottom": 72, "left": 72},
      "children": [
        {
          "type": "text",
          "runs": [{"text": "Hello, Inkoxide", "font": "Inter-Regular", "size": 24}]
        }
      ]
    }
  ]
}
```

## 2. Invoice: table, header and footer

- **Header and footer.** They belong to the page template: they are laid
  out once at the full page width, then painted on every physical page.
  The page margins leave room for them.
- **Table.** It has a flexible description column and two fixed columns.
  The first row is a header row (`headerRows: 1`), which repeats if the
  table splits across pages. The total row uses `colSpan`.
- **Creation date.** `meta.creationDate` makes the output byte-for-byte
  reproducible.

```json
{
  "version": 1,
  "meta": {
    "title": "Invoice INV-0001",
    "author": "Example Ltd",
    "lang": "en",
    "creationDate": "2026-10-08T00:00:00Z"
  },
  "fonts": [
    {"id": "Inter-Regular", "family": "Inter"},
    {"id": "Inter-Bold", "family": "Inter", "weight": 700}
  ],
  "pages": [
    {
      "size": {"width": 595.28, "height": 841.89},
      "margin": {"top": 90, "right": 48, "bottom": 70, "left": 48},
      "header": {
        "type": "box",
        "style": {
          "flexDirection": "row",
          "justifyContent": "space-between",
          "padding": {"top": 32, "right": 48, "left": 48}
        },
        "children": [
          {
            "type": "text",
            "runs": [{"text": "Example Ltd", "font": "Inter-Bold", "size": 14}]
          },
          {
            "type": "text",
            "runs": [
              {
                "text": "INVOICE INV-0001",
                "font": "Inter-Bold",
                "size": 14,
                "color": [0.16, 0.36, 0.86, 1]
              }
            ]
          }
        ]
      },
      "footer": {
        "type": "box",
        "style": {"padding": {"right": 48, "bottom": 28, "left": 48}},
        "children": [
          {
            "type": "text",
            "style": {"textAlign": "center"},
            "runs": [
              {
                "text": "Example Ltd · 25 Riverside Avenue, Cork · Company No. 654321",
                "font": "Inter-Regular",
                "size": 8,
                "color": [0.42, 0.45, 0.52, 1]
              }
            ]
          }
        ]
      },
      "children": [
        {
          "type": "text",
          "style": {"margin": {"bottom": 16}},
          "runs": [
            {"text": "Billed to: ", "font": "Inter-Bold", "size": 10},
            {"text": "Studio North Ltd, Dublin", "font": "Inter-Regular", "size": 10}
          ]
        },
        {
          "type": "table",
          "headerRows": 1,
          "columns": [{"width": "1fr"}, {"width": 50}, {"width": 90}],
          "rows": [
            {
              "backgroundColor": [0.95, 0.96, 0.99, 1],
              "cells": [
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "left"},
                      "runs": [{"text": "Description", "font": "Inter-Bold", "size": 9}]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "Qty", "font": "Inter-Bold", "size": 9}]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [
                        {"text": "Amount (EUR)", "font": "Inter-Bold", "size": 9}
                      ]
                    }
                  ]
                }
              ]
            },
            {
              "cells": [
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "left"},
                      "runs": [
                        {"text": "Engine licence", "font": "Inter-Regular", "size": 9}
                      ]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "1", "font": "Inter-Regular", "size": 9}]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "1,200.00", "font": "Inter-Regular", "size": 9}]
                    }
                  ]
                }
              ]
            },
            {
              "cells": [
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "left"},
                      "runs": [
                        {
                          "text": "Integration support (hours)",
                          "font": "Inter-Regular",
                          "size": 9
                        }
                      ]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "6", "font": "Inter-Regular", "size": 9}]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "570.00", "font": "Inter-Regular", "size": 9}]
                    }
                  ]
                }
              ]
            },
            {
              "cells": [
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "left"},
                      "runs": [
                        {
                          "text": "Training workshop",
                          "font": "Inter-Regular",
                          "size": 9
                        }
                      ]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "1", "font": "Inter-Regular", "size": 9}]
                    }
                  ]
                },
                {
                  "style": {
                    "padding": {"top": 4, "right": 6, "bottom": 4, "left": 6},
                    "borderWidth": {"bottom": 0.5},
                    "borderColor": {"bottom": [0.85, 0.86, 0.9, 1]}
                  },
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "650.00", "font": "Inter-Regular", "size": 9}]
                    }
                  ]
                }
              ]
            },
            {
              "cells": [
                {
                  "colSpan": 2,
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right", "padding": {"top": 6, "right": 6}},
                      "runs": [{"text": "Total", "font": "Inter-Bold", "size": 10}]
                    }
                  ]
                },
                {
                  "children": [
                    {
                      "type": "text",
                      "style": {"textAlign": "right", "padding": {"top": 6, "right": 6}},
                      "runs": [{"text": "2,420.00", "font": "Inter-Bold", "size": 10}]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

## 3. Multi-page report with page numbers

- **Flow.** The body overflows onto as many pages as it needs.
- **Section headings.** `break.minPresenceAhead` keeps each heading from
  being stranded at the bottom of a page.
- **Explicit break.** A `pageBreak` node starts the appendix on a new
  page.
- **Footer.** It shows "Page N of M" through the `pageNumber` and
  `totalPages` placeholders, which are resolved after pagination.
- **Hyphenation.** `hyphenate: true` breaks words at language patterns
  for `meta.lang`; soft hyphens are always honoured.

```json
{
  "version": 1,
  "meta": {"title": "Quarterly report", "lang": "en"},
  "fonts": [
    {"id": "Inter-Regular", "family": "Inter"},
    {"id": "Inter-Bold", "family": "Inter", "weight": 700}
  ],
  "pages": [
    {
      "size": {"width": 595.28, "height": 841.89},
      "margin": {"top": 48, "right": 48, "bottom": 56, "left": 48},
      "footer": {
        "type": "box",
        "style": {
          "flexDirection": "row",
          "justifyContent": "space-between",
          "padding": {"top": 12, "right": 48, "bottom": 20, "left": 48}
        },
        "children": [
          {
            "type": "text",
            "runs": [
              {
                "text": "Quarterly report",
                "font": "Inter-Regular",
                "size": 8,
                "color": [0.42, 0.45, 0.52, 1]
              }
            ]
          },
          {
            "type": "text",
            "runs": [
              {
                "text": "Page ",
                "font": "Inter-Regular",
                "size": 8,
                "color": [0.42, 0.45, 0.52, 1]
              },
              {
                "placeholder": "pageNumber",
                "font": "Inter-Regular",
                "size": 8,
                "color": [0.42, 0.45, 0.52, 1]
              },
              {
                "text": " of ",
                "font": "Inter-Regular",
                "size": 8,
                "color": [0.42, 0.45, 0.52, 1]
              },
              {
                "placeholder": "totalPages",
                "font": "Inter-Regular",
                "size": 8,
                "color": [0.42, 0.45, 0.52, 1]
              }
            ]
          }
        ]
      },
      "children": [
        {
          "type": "text",
          "style": {"margin": {"bottom": 12}},
          "runs": [{"text": "Quarterly report", "font": "Inter-Bold", "size": 24}]
        },
        {
          "type": "text",
          "break": {"minPresenceAhead": 60},
          "style": {"margin": {"top": 10, "bottom": 6}},
          "runs": [{"text": "1. Section 1", "font": "Inter-Bold", "size": 15}]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "break": {"minPresenceAhead": 60},
          "style": {"margin": {"top": 10, "bottom": 6}},
          "runs": [{"text": "2. Section 2", "font": "Inter-Bold", "size": 15}]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "break": {"minPresenceAhead": 60},
          "style": {"margin": {"top": 10, "bottom": 6}},
          "runs": [{"text": "3. Section 3", "font": "Inter-Bold", "size": 15}]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {
          "type": "text",
          "style": {
            "textAlign": "justify",
            "lineHeight": 1.45,
            "hyphenate": true,
            "margin": {"bottom": 8}
          },
          "runs": [
            {
              "text": "Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter. Quarterly revenue grew in every region, driven by retention of existing customers and a steady flow of new contracts in the public sector. Operating costs stayed within budget; the largest variance came from infrastructure investment brought forward from the next quarter.",
              "font": "Inter-Regular",
              "size": 10
            }
          ]
        },
        {"type": "pageBreak"},
        {
          "type": "text",
          "runs": [{"text": "Appendix", "font": "Inter-Bold", "size": 15}]
        }
      ]
    }
  ]
}
```
