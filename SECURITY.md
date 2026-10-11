# Security policy

## Supported versions

Security fixes are made for the latest release of each package.

## Reporting a vulnerability

Please do not open a public issue. Report it privately through
[GitHub's private vulnerability reporting](https://github.com/eddiesr93/inkoxide/security/advisories/new),
with the input that triggers it and the version you used. You will get a reply within a week.

## Untrusted input

Inkoxide is designed to render documents built from untrusted data:

- The engine is written in safe Rust (`#![forbid(unsafe_code)]` in the core crate) and is fuzzed
  with JSON, structured documents and SVG images as input.
- Documents are validated before rendering, and page counts, nesting depth, image sizes and SVG
  complexity are limited, so hostile input fails with an error instead of exhausting memory.
- `@inkoxide/render` does not fetch remote images unless the caller allows their origins.

A crash, a hang, unbounded memory use or a network request that the caller did not allow is a
security issue. Please report it as above.
