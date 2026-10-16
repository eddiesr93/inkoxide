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
- Documents are validated before rendering, and page counts, nesting depth, image sizes, SVG
  complexity and the colour glyphs drawn from fonts are limited, so hostile input fails with an error instead of exhausting memory.
- `@inkoxide/render` does not fetch remote images unless the caller allows their origins (an
  entry without a port allows only its scheme's default port), and does not follow a redirect from an allowed URL, since its target could be outside them. Each
  download has a timeout and a size limit. It does not read image files unless the caller allows their folders (`localImages`). An allowed
  path must lead, symbolic links followed, to a regular file inside one of those folders and
  within the size limit; network paths are refused. Errors name the path as the template wrote
  it, and do not tell a missing file from one outside the folders.
- A function or promise `<Image src>` that does not settle in time fails the render.
- In a browser (`@inkoxide/wasm`, `@inkoxide/render/browser`) the engine and its limits are the
  same, and an instance has at most 4 GiB of memory. Renders run in a Web Worker, which a
  `signal` ends. Remote images stay off unless allowed, as on a server, since the requests come
  from the user's network; they are sent without cookies or credentials. The page needs
  `script-src 'wasm-unsafe-eval'`, and nothing needs `'unsafe-eval'`.

## Encryption

An encrypted PDF (the `encryption` option, or `<Document userPassword>`) keeps its strings and
streams, so its text, images and form field values from whoever lacks a password: the text of
text fields, and the options of select, list and radio fields with the values chosen. What it
does not do:

- **Hide the structure of the file.** The page count and sizes, font names and glyph widths,
  where links and fields are, and the length of every stream and string stay readable, as in
  any encrypted PDF. Stream lengths are measured after compression, so they say something about
  the content: the length of a font subset, roughly how many glyphs it holds. The subset tags
  of font names are keyed, and the widths cover every glyph of the font, so neither tells which
  glyphs the file uses.
- **Hide which form choices are made.** PDF writes this state as names and numbers, which are
  never encrypted: whether each checkbox is checked, which radio button of a group is on (by a
  position in the group, not its value), and which options of a list box are selected and
  scrolled to (by their positions in the list). Anyone who knows the form's layout, from a blank
  copy for instance, can read the choices from these.
- **Protect anything without a user password.** Without one, anyone opens the file; the owner
  password then only guards the permissions, which readers honour and the file cannot enforce.
- **Resist weak passwords.** Anyone with the file can test password guesses offline, at the
  cost revision 6 sets, which is low. Use a long, random user password.
- **Detect changes.** PDF encryption has no integrity check: a changed file still opens.
- **Hide equal documents.** The keys, salts and IVs are derived from the document and the
  passwords rather than drawn at random, so the same input gives the same file. A service that
  encrypts documents it is asked to, with a fixed password, lets whoever asks confirm a guess
  of another file encrypted with that password by comparing the bytes.

The derivation is not the standard's random one. It has been reviewed internally but has not
had an independent cryptography review yet. Passwords never appear in errors, logs or `Debug`
output.

A crash, a hang, unbounded memory use, a network request or a file read that the caller did not
allow is a security issue. Please report it as above.
