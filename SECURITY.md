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
  every night with JSON, structured documents, SVG images and fonts as input.
- Documents are validated before rendering, and page counts, nesting depth, image sizes and SVG
  complexity are limited, so hostile input fails with an error instead of exhausting memory.
- `@inkoxide/render` does not fetch remote images unless the caller allows their origins, and
  does not follow a redirect from an allowed URL, since its target could be outside them. Each
  download has a timeout and a size limit. It does not read image files unless the caller allows their folders (`localImages`). An allowed
  path must lead, symbolic links followed, to a regular file inside one of those folders and
  within the size limit; network paths are refused. Errors name the path as the template wrote
  it, and do not tell a missing file from one outside the folders.
- A function or promise `<Image src>` that does not settle in time fails the render.

A crash, a hang, unbounded memory use, a network request or a file read that the caller did not
allow is a security issue. Please report it as above.
