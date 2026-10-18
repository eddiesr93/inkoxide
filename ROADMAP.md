# Roadmap

What is planned, in rough order. Plans change: open an issue if something you need is missing
or should come sooner.

## Later

- **In the browser**: `renderToStream`; reading local files; threads from a CDN, with the pool's
  worker served from another origin; a module with threads on stable Rust, once its standard
  library can be rebuilt there.
- **The Tailwind build plugin** for webpack, Rspack and Next.js (Turbopack), and reading a
  theme's `@source` directives (Vite, Rollup, Rolldown and esbuild have it now).
- **Encryption**: an independent review of the key derivation by cryptographers. It has had an
  internal review only.
