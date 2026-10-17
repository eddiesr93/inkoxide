# Schema examples

Complete documents for schema version 1. Each one validates and renders
as is, once the fonts it declares and the images it names are
registered: the files of `fixtures/fonts/` and `fixtures/images/`, under
their names without the extension (`Inter-Regular`, `logo`), as
`inkoxide render --fonts fixtures/fonts --images fixtures/images` registers
them. The Node tests render every JSON block in this file and
`scripts/check-schema.py` validates them, so they cannot go stale.

1. [Minimal single page](#1-minimal-single-page)
2. [Invoice: table, header and footer](#2-invoice-table-header-and-footer)
3. [Multi-page report with page numbers](#3-multi-page-report-with-page-numbers)
4. [Expense claim: a fillable form](#4-expense-claim-a-fillable-form)
5. [Newsletter: grid areas, a float and a background image](#5-newsletter-grid-areas-a-float-and-a-background-image)
6. [Menu card: inline images, fallback fonts and font features](#6-menu-card-inline-images-fallback-fonts-and-font-features)

All lengths are PDF points (1/72 in) and colours are RGBA floats in
[0, 1]. The full reference is [`docs/SCHEMA.md`](../../docs/SCHEMA.md).

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

## 4. Expense claim: a fillable form

- **Fields.** Each `field` node is an AcroForm field that viewers let the
  reader fill in. Its `style` sizes and decorates the box; the widget
  covers the content box. Names with dots (`claimant.name`) group fields.
- **Formats.** The date and the amount are text fields with a `format`:
  the field shows `2026-10-12` as `12 October 2026` and `1284.5` as
  `€ 1,284.50`, and the viewer formats what the reader types the same way.
- **Choices.** `select` is a drop-down list; `list` with `multiSelect`
  shows several options and keeps several selected. The two `radio`
  buttons share the `name` `refund`, so they form one group; each exports
  its own `value`.
- **Note.** A `note` node is a sticky note in the corner of its box:
  viewers show its text when it is opened.

```json
{
  "version": 1,
  "meta": {"title": "Expense claim", "lang": "en"},
  "fonts": [
    {"id": "Inter-Regular", "family": "Inter"},
    {"id": "Inter-Bold", "family": "Inter", "weight": 700}
  ],
  "pages": [
    {
      "size": {"width": 595.28, "height": 841.89},
      "margin": {"top": 56, "right": 56, "bottom": 56, "left": 56},
      "children": [
        {
          "type": "box",
          "style": {"flexDirection": "row", "justifyContent": "space-between", "margin": {"bottom": 20}},
          "children": [
            {
              "type": "text",
              "runs": [{"text": "Expense claim", "font": "Inter-Bold", "size": 20}]
            },
            {
              "type": "box",
              "style": {"width": 20, "height": 20},
              "children": [
                {
                  "type": "note",
                  "note": {
                    "contents": "Attach a receipt for every item over EUR 50.",
                    "author": "Finance",
                    "open": false
                  }
                }
              ]
            }
          ]
        },
        {
          "type": "box",
          "style": {"flexDirection": "row", "gap": 16, "margin": {"bottom": 12}},
          "children": [
            {
              "type": "box",
              "style": {"flexGrow": 1, "gap": 3},
              "children": [
                {"type": "text", "runs": [{"text": "Full name", "font": "Inter-Bold", "size": 8}]},
                {
                  "type": "field",
                  "style": {
                    "height": 20,
                    "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                    "borderColor": {
                      "top": [0.7, 0.72, 0.78, 1],
                      "right": [0.7, 0.72, 0.78, 1],
                      "bottom": [0.7, 0.72, 0.78, 1],
                      "left": [0.7, 0.72, 0.78, 1]
                    }
                  },
                  "field": {
                    "name": "claimant.name",
                    "kind": "text",
                    "value": "Aoife Byrne",
                    "font": "Inter-Regular",
                    "size": 10,
                    "required": true
                  }
                }
              ]
            },
            {
              "type": "box",
              "style": {"width": 140, "gap": 3},
              "children": [
                {"type": "text", "runs": [{"text": "Date of travel", "font": "Inter-Bold", "size": 8}]},
                {
                  "type": "field",
                  "style": {
                    "height": 20,
                    "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                    "borderColor": {
                      "top": [0.7, 0.72, 0.78, 1],
                      "right": [0.7, 0.72, 0.78, 1],
                      "bottom": [0.7, 0.72, 0.78, 1],
                      "left": [0.7, 0.72, 0.78, 1]
                    }
                  },
                  "field": {
                    "name": "claimant.date",
                    "kind": "text",
                    "value": "2026-10-12",
                    "font": "Inter-Regular",
                    "size": 10,
                    "format": {"type": "date", "pattern": "d mmmm yyyy"}
                  }
                }
              ]
            },
            {
              "type": "box",
              "style": {"width": 110, "gap": 3},
              "children": [
                {"type": "text", "runs": [{"text": "Amount", "font": "Inter-Bold", "size": 8}]},
                {
                  "type": "field",
                  "style": {
                    "height": 20,
                    "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                    "borderColor": {
                      "top": [0.7, 0.72, 0.78, 1],
                      "right": [0.7, 0.72, 0.78, 1],
                      "bottom": [0.7, 0.72, 0.78, 1],
                      "left": [0.7, 0.72, 0.78, 1]
                    }
                  },
                  "field": {
                    "name": "claimant.amount",
                    "kind": "text",
                    "value": "1284.5",
                    "font": "Inter-Regular",
                    "size": 10,
                    "format": {"type": "number", "decimals": 2, "currency": "€ ", "negative": "parens"}
                  }
                }
              ]
            }
          ]
        },
        {
          "type": "box",
          "style": {"flexDirection": "row", "gap": 16, "margin": {"bottom": 16}},
          "children": [
            {
              "type": "box",
              "style": {"flexGrow": 1, "gap": 3},
              "children": [
                {"type": "text", "runs": [{"text": "Department", "font": "Inter-Bold", "size": 8}]},
                {
                  "type": "field",
                  "style": {
                    "height": 20,
                    "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                    "borderColor": {
                      "top": [0.7, 0.72, 0.78, 1],
                      "right": [0.7, 0.72, 0.78, 1],
                      "bottom": [0.7, 0.72, 0.78, 1],
                      "left": [0.7, 0.72, 0.78, 1]
                    }
                  },
                  "field": {
                    "name": "department",
                    "kind": "select",
                    "options": ["Engineering", "Sales", "Operations", "Finance"],
                    "value": "Sales",
                    "font": "Inter-Regular",
                    "size": 10
                  }
                }
              ]
            },
            {
              "type": "box",
              "style": {"flexGrow": 1, "gap": 3},
              "children": [
                {
                  "type": "text",
                  "runs": [{"text": "Categories (select all that apply)", "font": "Inter-Bold", "size": 8}]
                },
                {
                  "type": "field",
                  "style": {
                    "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                    "borderColor": {
                      "top": [0.7, 0.72, 0.78, 1],
                      "right": [0.7, 0.72, 0.78, 1],
                      "bottom": [0.7, 0.72, 0.78, 1],
                      "left": [0.7, 0.72, 0.78, 1]
                    }
                  },
                  "field": {
                    "name": "categories",
                    "kind": "list",
                    "options": ["Travel", "Meals", "Lodging", "Equipment"],
                    "value": ["Travel", "Lodging"],
                    "multiSelect": true,
                    "font": "Inter-Regular",
                    "size": 10
                  }
                }
              ]
            }
          ]
        },
        {"type": "text", "runs": [{"text": "Refund to", "font": "Inter-Bold", "size": 8}]},
        {
          "type": "box",
          "style": {"flexDirection": "row", "alignItems": "center", "gap": 6, "margin": {"top": 4, "bottom": 16}},
          "children": [
            {
              "type": "field",
              "style": {
                "borderRadius": 7, "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                "borderColor": {
                  "top": [0.7, 0.72, 0.78, 1],
                  "right": [0.7, 0.72, 0.78, 1],
                  "bottom": [0.7, 0.72, 0.78, 1],
                  "left": [0.7, 0.72, 0.78, 1]
                }
              },
              "field": {"name": "refund", "kind": "radio", "value": "bank", "checked": true, "size": 10}
            },
            {
              "type": "text",
              "style": {"margin": {"right": 12}},
              "runs": [{"text": "Bank account on file", "font": "Inter-Regular", "size": 10}]
            },
            {
              "type": "field",
              "style": {
                "borderRadius": 7, "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                "borderColor": {
                  "top": [0.7, 0.72, 0.78, 1],
                  "right": [0.7, 0.72, 0.78, 1],
                  "bottom": [0.7, 0.72, 0.78, 1],
                  "left": [0.7, 0.72, 0.78, 1]
                }
              },
              "field": {"name": "refund", "kind": "radio", "value": "card", "size": 10}
            },
            {
              "type": "text",
              "runs": [{"text": "Company card", "font": "Inter-Regular", "size": 10}]
            }
          ]
        },
        {
          "type": "box",
          "style": {"flexDirection": "row", "alignItems": "center", "gap": 6},
          "children": [
            {
              "type": "field",
              "style": {
                "borderWidth": {"top": 0.75, "right": 0.75, "bottom": 0.75, "left": 0.75},
                "borderColor": {
                  "top": [0.7, 0.72, 0.78, 1],
                  "right": [0.7, 0.72, 0.78, 1],
                  "bottom": [0.7, 0.72, 0.78, 1],
                  "left": [0.7, 0.72, 0.78, 1]
                }
              },
              "field": {"name": "confirm", "kind": "checkbox", "required": true, "size": 10}
            },
            {
              "type": "text",
              "runs": [
                {
                  "text": "I confirm that these expenses were incurred on company business.",
                  "font": "Inter-Regular",
                  "size": 10
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

## 5. Newsletter: grid areas, a float and a background image

- **Grid areas.** `gridTemplateAreas` names the cells of the grid, and
  each child takes one with `gridArea`: the banner spans both columns, the
  sidebar and the article share the second row.
- **Background image.** The banner draws the image `photo` behind its text
  with `fit: "cover"`, clipped to its rounded corners.
- **Float.** The chart floats right inside the article, and the text after
  it wraps beside it. `clear: "both"` puts the last box below it.
- **Even lines.** `textWrap: "pretty"` chooses the line breaks of each
  paragraph as a whole, for lines of even length, with hyphenation in
  `meta.lang`.
- **Tabular figures.** `fontFeatures: {"tnum": 1}` gives the rainfall
  figures digits of one width, so they line up.

```json
{
  "version": 1,
  "meta": {"title": "Field notes, autumn issue", "lang": "en"},
  "fonts": [
    {"id": "Inter-Regular", "family": "Inter"},
    {"id": "Inter-Bold", "family": "Inter", "weight": 700}
  ],
  "pages": [
    {
      "size": {"width": 595.28, "height": 841.89},
      "margin": {"top": 48, "right": 48, "bottom": 48, "left": 48},
      "children": [
        {
          "type": "box",
          "style": {
            "display": "grid",
            "gridTemplateAreas": ["banner banner", "side main"],
            "gridTemplateColumns": [130, "1fr"],
            "columnGap": 20,
            "rowGap": 16
          },
          "children": [
            {
              "type": "box",
              "style": {
                "gridArea": "banner",
                "height": 120,
                "justifyContent": "flex-end",
                "padding": {"right": 16, "bottom": 14, "left": 16},
                "borderRadius": 8
              },
              "backgroundImage": {"src": "photo", "fit": "cover", "objectPosition": {"x": "center", "y": "30%"}},
              "children": [
                {
                  "type": "text",
                  "runs": [{"text": "Field notes", "font": "Inter-Bold", "size": 26, "color": [1, 1, 1, 1]}]
                },
                {
                  "type": "text",
                  "runs": [
                    {"text": "Autumn issue · October 2026", "font": "Inter-Regular", "size": 10, "color": [1, 1, 1, 1]}
                  ]
                }
              ]
            },
            {
              "type": "box",
              "style": {
                "gridArea": "side",
                "gap": 10,
                "padding": {"top": 10, "right": 10, "bottom": 10, "left": 10},
                "backgroundColor": [0.95, 0.96, 0.98, 1]
              },
              "children": [
                {"type": "text", "runs": [{"text": "In this issue", "font": "Inter-Bold", "size": 10}]},
                {
                  "type": "text",
                  "style": {"lineHeight": 1.4},
                  "runs": [
                    {"text": "Soil moisture across the three trial plots, and what the dry September did to them.", "font": "Inter-Regular", "size": 9}
                  ]
                },
                {"type": "text", "runs": [{"text": "Rainfall (mm)", "font": "Inter-Bold", "size": 10}]},
                {
                  "type": "box",
                  "style": {"display": "grid", "gridTemplateColumns": ["1fr", "auto"], "rowGap": 3},
                  "children": [
                    {"type": "text", "runs": [{"text": "July", "font": "Inter-Regular", "size": 9}]},
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "41.2", "font": "Inter-Regular", "size": 9, "fontFeatures": {"tnum": 1}}]
                    },
                    {"type": "text", "runs": [{"text": "August", "font": "Inter-Regular", "size": 9}]},
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "18.7", "font": "Inter-Regular", "size": 9, "fontFeatures": {"tnum": 1}}]
                    },
                    {"type": "text", "runs": [{"text": "September", "font": "Inter-Regular", "size": 9}]},
                    {
                      "type": "text",
                      "style": {"textAlign": "right"},
                      "runs": [{"text": "6.0", "font": "Inter-Regular", "size": 9, "fontFeatures": {"tnum": 1}}]
                    }
                  ]
                }
              ]
            },
            {
              "type": "box",
              "style": {"gridArea": "main"},
              "children": [
                {
                  "type": "image",
                  "src": "chart",
                  "style": {"float": "right", "width": 150, "height": 100, "margin": {"left": 12, "bottom": 8}}
                },
                {
                  "type": "text",
                  "style": {"margin": {"bottom": 6}},
                  "runs": [{"text": "A dry month on the north plots", "font": "Inter-Bold", "size": 14}]
                },
                {
                  "type": "text",
                  "style": {"textAlign": "justify", "textWrap": "pretty", "hyphenate": true, "lineHeight": 1.45, "margin": {"bottom": 8}},
                  "runs": [
                    {
                      "text": "September brought six millimetres of rain to the north plots, the lowest total since measurements began. The sensors at thirty centimetres show the soil drying steadily from the second week, while the sheltered south plot held its moisture until the last few days of the month. The chart beside this paragraph compares the three plots week by week.",
                      "font": "Inter-Regular",
                      "size": 10
                    }
                  ]
                },
                {
                  "type": "text",
                  "style": {"textAlign": "justify", "textWrap": "pretty", "hyphenate": true, "lineHeight": 1.45, "margin": {"bottom": 8}},
                  "runs": [
                    {
                      "text": "Irrigation started on the twentieth, at half the usual volume, to see how quickly the deeper layers recover. Early readings are encouraging: the top layer was back to its August level within four days, although the deeper sensors have barely moved. We will report on the recovery, and on the winter cover crops, in the next issue.",
                      "font": "Inter-Regular",
                      "size": 10
                    }
                  ]
                },
                {
                  "type": "box",
                  "style": {"clear": "both", "borderWidth": {"top": 0.5}, "borderColor": {"top": [0.8, 0.82, 0.86, 1]}, "padding": {"top": 6}},
                  "children": [
                    {
                      "type": "text",
                      "runs": [
                        {"text": "Readings are daily means from the plot sensors.", "font": "Inter-Regular", "size": 8, "color": [0.42, 0.45, 0.52, 1]}
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
  ]
}
```

## 6. Menu card: inline images, fallback fonts and font features

- **Inline image.** The logo is a run with an `image` instead of `text`:
  1em high by default, centred on the line with `verticalAlign: "middle"`,
  and copied as its `alt` text.
- **Fallback fonts.** Inter has no Arabic letters, so the Arabic name
  is drawn with the first font of `fallbackFonts` that has them. The
  paragraph is still left to right; the Arabic words are ordered right to
  left within it.
- **Superscript and subscript.** `verticalAlign: "super"` and `"sub"` move
  a run's baseline by the font's own offsets; the run sets its own smaller
  `size`.
- **Emoji images.** `emojiImages` draws 😀 with the image `emoji-smile`,
  1em high, wherever the text has it; copying the text still gives the
  emoji.
- **Font features.** The prices use `tnum`, so they line up in the
  right-aligned column.

```json
{
  "version": 1,
  "meta": {"title": "Harbour Café menu", "lang": "en"},
  "fonts": [
    {"id": "Inter-Regular", "family": "Inter"},
    {"id": "Inter-Bold", "family": "Inter", "weight": 700},
    {"id": "NotoSansArabic-Regular", "family": "Noto Sans Arabic"}
  ],
  "fallbackFonts": ["NotoSansArabic-Regular"],
  "emojiImages": {"😀": "emoji-smile"},
  "pages": [
    {
      "size": {"width": 419.53, "height": 595.28},
      "margin": {"top": 40, "right": 40, "bottom": 40, "left": 40},
      "children": [
        {
          "type": "text",
          "style": {"margin": {"bottom": 4}},
          "runs": [
            {"font": "Inter-Bold", "size": 22, "image": {"src": "logo", "alt": "Harbour Café logo"}, "verticalAlign": "middle"},
            {"text": " Harbour Café", "font": "Inter-Bold", "size": 22}
          ]
        },
        {
          "type": "text",
          "style": {"margin": {"bottom": 18}},
          "runs": [
            {"text": "مقهى الميناء · open daily from 8:00", "font": "Inter-Regular", "size": 10, "color": [0.42, 0.45, 0.52, 1]}
          ]
        },
        {
          "type": "box",
          "style": {"display": "grid", "gridTemplateColumns": ["1fr", "auto"], "rowGap": 8, "columnGap": 12},
          "children": [
            {
              "type": "text",
              "runs": [
                {"text": "Flat white", "font": "Inter-Regular", "size": 11},
                {"text": "1", "font": "Inter-Regular", "size": 7, "verticalAlign": "super"}
              ]
            },
            {
              "type": "text",
              "style": {"textAlign": "right"},
              "runs": [{"text": "€3.40", "font": "Inter-Regular", "size": 11, "fontFeatures": {"tnum": 1}}]
            },
            {
              "type": "text",
              "runs": [
                {"text": "Sparkling water (H", "font": "Inter-Regular", "size": 11},
                {"text": "2", "font": "Inter-Regular", "size": 7, "verticalAlign": "sub"},
                {"text": "O, 330 ml)", "font": "Inter-Regular", "size": 11}
              ]
            },
            {
              "type": "text",
              "style": {"textAlign": "right"},
              "runs": [{"text": "€2.10", "font": "Inter-Regular", "size": 11, "fontFeatures": {"tnum": 1}}]
            },
            {
              "type": "text",
              "runs": [
                {"text": "Almond croissant", "font": "Inter-Regular", "size": 11},
                {"text": "1, 2", "font": "Inter-Regular", "size": 7, "verticalAlign": "super"}
              ]
            },
            {
              "type": "text",
              "style": {"textAlign": "right"},
              "runs": [{"text": "€3.20", "font": "Inter-Regular", "size": 11, "fontFeatures": {"tnum": 1}}]
            }
          ]
        },
        {
          "type": "text",
          "style": {"margin": {"top": 18}, "lineHeight": 1.4},
          "runs": [
            {"text": "1", "font": "Inter-Regular", "size": 6, "verticalAlign": "super", "color": [0.42, 0.45, 0.52, 1]},
            {"text": " Contains milk.  ", "font": "Inter-Regular", "size": 8, "color": [0.42, 0.45, 0.52, 1]},
            {"text": "2", "font": "Inter-Regular", "size": 6, "verticalAlign": "super", "color": [0.42, 0.45, 0.52, 1]},
            {"text": " Contains nuts. Prices include VAT.", "font": "Inter-Regular", "size": 8, "color": [0.42, 0.45, 0.52, 1]}
          ]
        },
        {
          "type": "text",
          "style": {"margin": {"top": 12}},
          "runs": [{"text": "Thank you for visiting 😀", "font": "Inter-Bold", "size": 11}]
        }
      ]
    }
  ]
}
```
