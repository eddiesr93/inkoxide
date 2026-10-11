# Examples

Seven complete documents for `@inkoxide/render`, one file each. Every file exports its component
and sample data, and renders `<name>.pdf` when you run it:

```sh
npx tsx invoice.tsx
```

| File | Shows |
|---|---|
| `receipt.tsx` | The README's quick start: an A6 receipt in 30 lines |
| `invoice.tsx` | An SVG logo, a table with a repeating header, totals kept together, page numbers |
| `report.tsx` | KPI cards, an SVG bar chart, a 60-row table across pages, a header from page 2, bookmarks |
| `certificate.tsx` | A landscape page, nested borders, Inter Display, a drawn seal, an absolute serial number |
| `letter.tsx` | US Letter, justified and hyphenated paragraphs, a `mailto:` link, a footer rule |
| `resume.tsx` | A CSS grid with a full-height sidebar, wrapping tags, jobs kept on one page |
| `form.tsx` | Fillable text fields, a select, radio buttons and checkboxes |

`pnpm --filter @inkoxide/examples render` renders all of them to `out/`, writes the first page of
each to `packages/render/media/`, framed as a sheet of paper on a grey canvas, and rewrites the
quick start, gallery and examples sections of `packages/render/README.md` from these files, so the
README always shows the code that made each picture. It needs poppler (`pdftoppm`).
