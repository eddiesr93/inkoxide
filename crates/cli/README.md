# inkoxide-cli

The command line for the [Inkoxide](https://crates.io/crates/inkoxide) PDF engine: it renders and
validates document trees written as JSON or MessagePack.

```sh
cargo install inkoxide-cli
```

```sh
# Render: fonts and images are registered under their file stem
# (Inter-Regular.ttf is the font Inter-Regular).
inkoxide render invoice.json --fonts fonts/ --images images/ -o invoice.pdf

# Validate: prints every problem, with its path (--json for a JSON array).
# With --pdf-a, --pdf-ua, --flatten-alpha or --attachments, also what a
# render with them would refuse.
inkoxide validate invoice.json --fonts fonts/ --pdf-a=1b

# Render repeatedly and report timings.
inkoxide bench invoice.json --fonts fonts/ --iterations 50
```

`inkoxide render --help` lists the options: PDF/A (`--pdf-a=1b`, `2b`, `2a`, `3b` or `3a`, and
`--flatten-alpha`, which draws images with partial alpha on white at PDF/A-1b), tagged
PDF (`--tagged`) and PDF/UA-1 (`--pdf-ua`), the files of the document's attachments
(`--attachments <DIR>`, registered under their file name), the creation date, hyphenation patterns,
the number of threads, and PNG output through `pdftoppm`.

The document format is described in
[docs/SCHEMA.md](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md).

## Licence

MIT.
