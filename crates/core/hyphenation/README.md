# Built-in hyphenation patterns

The hyphenation automata of [hypher](https://github.com/typst/hypher) 0.1.8, one file a
language, copied unchanged from its `tries/` folder. `src/text/builtin.rs` lists them, and
matches words against them as hypher does; a test compares the two in every language.

The engine embeds them (the `embedded-hyphenation` cargo feature, on by default). The
WebAssembly engine leaves them out, and `@inkoxide/wasm` fetches a language's file when a
document needs it.

hypher builds the automata from the TeX patterns of
[hyph-utf8](https://github.com/hyphenation/tex-hyphen), and includes only patterns under
permissive licences: the LPPL, MPL, MIT and BSD-3-Clause. Each language's licence is in its
pattern file in hypher's repository (`patterns/`). hypher's code is under the MIT or
Apache-2.0 licence.
