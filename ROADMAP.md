# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Later

- **In the browser**: a module with threads on stable Rust, once its standard library can be
  rebuilt there.
- **Encryption**: an independent review of the key derivation by cryptographers. It has had an
  internal review only.
- **Svelte**: a custom renderer in place of reading back what Svelte rendered, once Svelte ships
  them ([sveltejs/svelte#18042](https://github.com/sveltejs/svelte/pull/18042)), so that component
  stacks name the template's own components.
- **Angular**: typed components beside the `ink-*` elements, so that templates are type-checked.
- **Examples**: a browser page for Vue, Svelte and Angular, with the Playwright tests of the React
  one.
