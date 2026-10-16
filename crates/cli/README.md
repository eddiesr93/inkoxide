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
inkoxide validate invoice.json --fonts fonts/

# Render repeatedly and report timings.
inkoxide bench invoice.json --fonts fonts/ --iterations 50
```

`inkoxide render --help` lists the options: PDF/A (`--pdf-a=1b`, `2b` or `3b`), the creation date,
hyphenation patterns, the number of threads, and PNG output through `pdftoppm`.

The document format is described in
[docs/SCHEMA.md](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md).

## Licence

MIT.
