# Domain docs

How the engineering skills use this repo's domain documentation when exploring the code.

## Before exploring, read these

- **`docs/adr/`**: the architecture decision records. Read the ones that touch the area you are
  about to work in.
- **`docs/ARCHITECTURE.md`**: how the repository is organised, and how a document goes from React
  to PDF.

## Flag ADR conflicts

If your output contradicts an ADR, say so explicitly rather than overriding it silently:

> _Contradicts ADR 0001 (SVG text is laid out by Inkoxide and drawn at probes), but worth
> reopening because…_

## Recording a decision

A new ADR goes in `docs/adr/`, numbered after the last one (`0003-<topic>.md`), with the
sections of the existing ones: a status line, Context, Decision, Consequences and Alternatives.
