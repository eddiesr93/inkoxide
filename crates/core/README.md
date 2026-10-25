# inkoxide

The Inkoxide PDF engine: a document tree in, PDF bytes out. It does the typesetting, so text
shaping, line breaking, flexbox and grid layout, pagination and PDF output. It does no I/O: you
give it the document, the fonts and the images as bytes.

This is the engine behind the npm packages [`inkoxide`](https://www.npmjs.com/package/inkoxide)
(Node.js), [`@inkoxide/wasm`](https://www.npmjs.com/package/@inkoxide/wasm) (browsers) and
[`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render), which renders templates of
React, Vue, Svelte and Angular.

```rust
use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"}],
      "pages":[{"size":{"width":595,"height":842},"children":[
        {"type":"text","runs":[{"text":"Hello from Inkoxide","font":"Inter-Regular","size":24}]}]}]}"#;

    let mut fonts = FontStore::new();
    fonts.register("Inter-Regular", std::fs::read("Inter-Regular.ttf")?)?;

    let options = RenderOptions::default();
    let pdf = inkoxide::render(doc.as_bytes(), InputFormat::Json, &options, &fonts, &ImageStore::new())?;
    std::fs::write("hello.pdf", pdf)?;
    Ok(())
}
```

The same input gives the same bytes: no date is written unless the document or the options set
one. `validate` reports the problems in a document, each with its path, and `render` never
returns a partial PDF.

## Features

- `msgpack` (default): MessagePack input, with the same structure as the JSON.
- `embedded-hyphenation` (default): the built-in TeX hyphenation patterns (1.2 MB). Without it,
  `HyphenationStore::register_builtin` gives them at run time.

## Documentation

- [The document schema](https://github.com/eddiesr93/inkoxide/blob/main/docs/SCHEMA.md): every
  node and style property.
- [Architecture](https://github.com/eddiesr93/inkoxide/blob/main/docs/ARCHITECTURE.md).
- [`inkoxide-cli`](https://crates.io/crates/inkoxide-cli) renders and validates documents from
  the command line.

## Licence

MIT.
