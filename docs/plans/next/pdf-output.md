# Plan: encryption and more PDF/A levels

Part of [Plans for "Next"](README.md). Draft for review; line references point to `f038fed`.

## Encryption (user and owner passwords, permissions)

**Scope and API**
- **Rust core:** a new field `RenderOptions.encryption: Option<Encryption>` with a `with_encryption` setter (lib.rs:73-116). New public input types (ARCHITECTURE.md:79-83 lists the public types and needs updating):
  - `Encryption { user_password, owner_password, permissions }`
  - `Permissions { printing: Option<Printing>, modifying, copying, annotating, filling_forms, content_accessibility, document_assembly }`
  - `Printing::{LowResolution, HighResolution}`

  All are `#[non_exhaustive]` with setters. Their `Debug` must be hand-written and print `<redacted>`, because `RenderOptions` derives `Debug` (lib.rs:73).
- **Node binding** (crates/node/src/lib.rs:121-142, packages/inkoxide/index.d.ts:44-75): `encryption?: { userPassword?, ownerPassword?, permissions?: { printing?: 'lowResolution'|'highResolution', modifying?, copying?, annotating?, fillingForms?, contentAccessibility?, documentAssembly? } }`. These are the names of the migration guide's `Permissions` type.
- **@inkoxide/render:** `RenderOpts.encryption` (types.ts:3-42), passed on in engine.ts:161-166.
- **React:** `<Document userPassword ownerPassword permissions>` (components/index.ts:300-327), for parity with MIGRATING.md:30. build.ts:317-339 moves these props into a new `BuildResult.output` (build.ts:82-98) whose `toJSON` redacts them. They never go into `document`, which is handed back in `RenderResult.document` (types.ts:104-105) to be stored. If `opts.encryption` is given it replaces the Document props as a whole, the same way `creationDate` overrides `meta.creationDate` (lib.rs:80-82).
- **Not in the schema:** a document never carries secrets, so packages/schema does not change.
- **Rules:**
  - At least one password is required; otherwise `INVALID_VALUE` at `options.encryption`.
  - `permissions` without an owner password is an error.
  - PDF/A together with encryption is `INVALID_VALUE` at `options.encryption`.
  - The Node binding maps `options.*` errors to `INVALID_ARGUMENT` (node lib.rs:60-70); the render package maps them to `ENGINE_REJECTED` (engine.ts:137-151).

**Algorithm:** AES-256 only: V5/R6, CFM `AESV3`, ISO 32000-2 Algorithms 2.A, 2.B, 8, 9 and 10.
- The header stays `%PDF-1.7` (pdf-writer writes it, pdf-writer lib.rs:245; core never calls `set_version`). Add `/Extensions /ADBE <</BaseVersion /1.7 /ExtensionLevel 8>>`; pdf-writer supports this (structure.rs:131).
- R6 is extension level 8. Level 3 is the withdrawn R5.
- RC4 is rejected because it is broken. AES-128 (R4) is rejected because it still needs MD5 and RC4 for O/U and is deprecated in PDF 2.0.
- R6 opens in Acrobat X+, pdf.js, PDFium, Preview, Poppler and qpdf.

**Engine design**
- pdf-writer 0.15 has no encryption support:
  - Strings are written by sealed `Primitive` impls (pdf-writer object.rs:10-23, 152-186).
  - The trailer has only `Root`, `Info` and `ID` (pdf-writer lib.rs:502-525).
1. **One module writes all strings and streams.** New `pdf/out.rs`:
   - `text(&str)` uses `TextStr`'s encoding: printable ASCII as is, anything else as FEFF + UTF-16BE. So `Str(&bytes)` is byte-identical to `TextStr` (pdf-writer object.rs:169-183, buf.rs:91-108), and unencrypted output does not change.
   - `bytes(&[u8])`, and `stream`/`image_xobject`/`form_xobject`/`tiling_pattern`/`cmap(r, data)`.
   - Typed setters that only accept `TextStr` become `.pair(Name(b"T"), Str(..))`.
2. **Call sites to move:**
   - Strings: mod.rs:433, 447-448, 482-498, 559, 638; font.rs:26-30 (CIDSystemInfo, used at 162 and 241-243); forms.rs:140, 162, 170, 194, 203.
   - Streams: mod.rs:387, 453, 455; font.rs:222, 231, 242; image.rs:28, 34, 59, 65; forms.rs:256; svg.rs:378, 893.
   - Content-stream `Str`s (mod.rs:788-816) stay as they are: the whole stream is encrypted.
3. **Guard:** a new `crates/core/clippy.toml`:
   - `disallowed-types` for `pdf_writer::TextStr`.
   - `disallowed-methods` for `Chunk::stream`, `image_xobject`, `form_xobject`, `tiling_pattern`, `cmap`, `metadata`, `icc_profile` and `embedded_file`.
   - Only out.rs is allowed to use them. CI runs clippy with `-D warnings` (ci.yml:53).
4. **`pdf/crypt.rs`:**
   - Hash 2.B, plus U/UE/O/OE/Perms.
   - AES-CBC with PKCS#5 padding and the IV prepended, written by hand over the `aes` block cipher.
   - HMAC-SHA256 (RFC 2104, about 15 lines).
5. **Changes to `write()`** (mod.rs:238-510):
   - Derive the keys first.
   - Encrypt content streams in the page loop (mod.rs:350-391).
   - Allocate the Encrypt object last, so every other object number matches the unencrypted file.
   - Call `set_file_id`.
   - Splice `/Encrypt n 0 R` into the trailer that `pdf.finish()` writes (mod.rs:509). The trailer comes after the xref table, so no offset moves (pdf-writer lib.rs:307-321). A unit test pins this. Upstream a `Pdf::set_encrypt` to pdf-writer and drop the splice once it is released.

**Determinism.** API.md:112-120 promises identical bytes for identical input, and fixtures.rs:80-93 and fuzz/src/lib.rs:93-98 check it across thread counts and JSON versus MessagePack. R6 asks for a random file key and salts, and random IVs would break all of those checks. Core has no RNG (no I/O, ARCHITECTURE.md:11), so derive everything instead:
- `D` = SHA-256 of the whole file as it is written unencrypted, so every object (images, links, fields, notes, the outline) feeds it. It does not depend on JSON vs MessagePack. `write()` writes the file twice: plain for `D`, then encrypted.
- Salts = HMAC(S_u‖S_o, "salts"‖D), with S_u = Hash2B(P_u, HMAC(D, "u")[..8]) and S_o = Hash2B(P_o, HMAC(D, "o")[..8]): they cannot be computed without the passwords, and checking a guess against them costs Algorithm 2.B hashes.
- `IK_u` = Hash2B(P_u, user key salt); `IK_o` = Hash2B(P_o, owner key salt, U).
- File key `K` = HMAC(IK_u‖IK_o, "key"‖D). Every guess still costs one R6 hash, the same as guessing through U or O.
- IV = HMAC(K_iv, position‖plaintext)[..16]. This is a synthetic IV, so a key and IV are never reused for different plaintexts. The position is the stream's object number, or a running counter for strings (`write()` is sequential).
- `/ID` = HMAC(K, "id")[..16]. It must not be the unkeyed xxh3 used for PDF/A (mod.rs:459-466): that would let anyone confirm a guessed plaintext.
- The 4 extra bytes in Perms = HMAC(K, "perms").

Result: identical inputs give identical files, and two different documents never share a key.

**Limits and security**
- Passwords are printable ASCII, at most 127 bytes. SASLprep leaves ASCII unchanged, so no NFKC tables are needed.
- Errors name the path (`options.encryption.userPassword`), never the value. The existing `creationDate` error does echo its input (lib.rs:289); don't copy that pattern.
- The render span logs only the byte count (lib.rs:226), so options are not traced.
- Document what encryption does not hide: dictionaries stay in plain text, so page count, page sizes, font names, `/W` widths (font.rs:182-197) and link rectangles are visible. That is also why `countPages` (engine.ts:178-200) keeps working.
- Fuzz oracle (fuzz/src/lib.rs:80-180), for a share of inputs rendered encrypted:
  - An encrypted render succeeds exactly when the plain render does.
  - A canary title never appears in plain text.
  - Stream scanning is skipped when the output is encrypted.

**Tests**
- KATs: HMAC against RFC 4231; R6 round trip (UE decrypts with the user password, OE with the owner password, Perms decrypts to `adb`).
- out.rs output is byte-identical to `TextStr`.
- New `tests/encryption.rs`:
  - Determinism across thread counts and MessagePack.
  - Every fixture encrypted passes `qpdf --check --password=…` (qpdf is installed in CI, ci.yml:44-45).
  - `--show-encryption` reports R6, AESv3 and the expected permission flags.
  - A wrong password fails; an owner-only file opens without a password.
  - **Oracle:** `qpdf --decrypt`, then `--json=2 --json-stream-data=inline`. The objects must equal the unencrypted render's, minus the last (Encrypt) object. This catches any string or stream left in plain text.
- A secret marker must be absent from errors, `Debug` output, `RenderError.toJSON` and TreeResult JSON.
- Node and Vitest: option shapes, the override rule, and the migration-guide props.

**Docs:**
- API.md:32-45 (options table) and 112-120 (Determinism).
- GUIDE.md:589-601, MIGRATING.md:30, CHANGELOG, ROADMAP.md:14.
- ARCHITECTURE.md:67 and 79-83 (public types; the out.rs rule).
- SECURITY.md: the threat model.

**New dependencies:**
- `aes` and `sha2` (RustCrypto, MIT OR Apache-2.0, allowed by deny.toml:21-32), pinned exactly like the other dependencies.
- Transitive: cipher, digest, crypto-common, block-buffer, inout, generic-array/hybrid-array, typenum and cpufeatures. `libc` and `cfg-if` are already in Cargo.lock:159 and 629.
- Not needed: cbc, hmac, rand, getrandom.
- The `unsafe` inside these crates does not conflict with `forbid(unsafe_code)`, which applies only to our crate.

**Effort:** L, about 7-8 days.

**Risks:**
- A string or stream site is missed. The clippy guard and the qpdf oracle cover this.
- The trailer splice could break on a pdf-writer upgrade.
- The non-standard deterministic key derivation needs a crypto review.
- Readers older than Acrobat X cannot open R6.

**Open decisions**
1. **Deterministic derivation or random keys.** Decided (2026-10-13): deterministic. An optional 32-byte `encryption.entropy`, for callers who want unlinkable output, can come later.
2. **Unicode passwords.** Recommend ASCII-only in v1; full SASLprep later (needs `unicode-normalization`).
3. **Missing owner password.** Recommend it defaults to the user password, as R2-R4 do; an empty owner password would let anyone get owner rights.
4. **Default permissions.** Recommend each permission is opt-in, as in the API in MIGRATING.md, except `contentAccessibility`, which defaults to true (PDF 2.0 deprecates that bit and says to always set it).
5. **Where passwords are accepted.** Recommend both the Document props and the render option, with the render option winning.

## More PDF/A levels (1b, 3b)

**Scope and API**
- **Rust core:** `RenderOptions.pdf_a_level: PdfALevel { A1b, A2b (default), A3b }`, `#[non_exhaustive]`. `with_pdf_a_level` also sets `pdf_a`. `pdf_a: bool` stays as it is, because API.md:9-11 makes a renamed option a major version.
- **Node and @inkoxide/render:** `pdfA?: boolean | '1b' | '2b' | '3b'`, where `true` means `'2b'`.
- **CLI:** `--pdf-a[=LEVEL]` (cli main.rs:85-87).
- **React:** `<Document conformance>` takes `'PDF/A-1'`, `'PDF/A-1b'`, `'PDF/A-2'`, `'PDF/A-2b'`, `'PDF/A-3'`, `'PDF/A-3b'`; a bare part number means level b. It is moved into `output` like the passwords. The a and u levels are rejected with a message pointing to tagged PDF (ROADMAP.md:30).

**Engine design**
- **Refactor first:** replace the `pdf_a: bool` that is threaded through mod.rs:81, 220, 317, 333, 345 and 440-478, forms.rs:228, image.rs:15 and svg.rs:51 and 356 with a `&Profile` carrying the level, header version, whether to write CIDSet, whether transparency is allowed, and the largest real number. Output stays byte-identical.
- **3b:** XMP `pdfaid:part` becomes 3 (make pdfa.rs:104 a parameter); everything else is 2b. Embedded files with `AFRelationship` come with File attachments (ROADMAP.md:31).
- **1b:**
  - `set_version(1,4)` and XMP part 1.
  - **CIDSet:** write a CIDSet stream in the FontDescriptor for subset fonts (font.rs:212-218 writes none today): used CIDs plus 0. Only for 1b, because the 2b profile requires a CIDSet to be complete when it is present.
  - **Transparency is forbidden.** The sources are soft masks (image.rs:57-74; svg.rs:384-385, 405-429, 773), alpha below 1 (mod.rs:254-264, 912-914) and blend modes. Reject them with the document path:
    - `Checker::color` (rules.rs:944) is the single place every colour's alpha is checked.
    - `style.opacity` (rules.rs:1161).
    - `debug`, which draws translucent outlines (paint/debug.rs:178, 350).
    - Image alpha, checked after `ImageSet::load` (lib.rs:306); fully opaque alpha is already dropped (decode.rs:178).
    - SVG: a scan of the usvg tree that mirrors the writer, plus a writer backstop that returns `PDF_ERROR` if transparency is reached anyway.
    - Add a new ValidationCode, e.g. `PDFA_CONFLICT`.
  - **Implementation limits (ISO 19005-1 §6.1.12):**
    - Reals within ±32767; today's bound is `MAX_NUMBER` = 1e11 (mod.rs:99).
    - Arrays of at most 8191 entries; the `/W` array (font.rs:182-197) and `Annots` can exceed this.
    - At most 4095 dictionary entries and 65535-byte strings.
    - `q` nesting of at most 28; `encode_page` already tracks saves (mod.rs:187).
    - Check these with pdf-writer's `Limits` (mod.rs:150-158, 504-508) and fail with `PDF_ERROR`, rather than clamping geometry.
  - **Already compliant:** plain xref table (mod.rs:509), ICC v2.1 (pdfa.rs:81), Print flag on annotations (mod.rs:547-549, forms.rs:301-305), no NeedAppearances (forms.rs:1-3), `.notdef` skipped (mod.rs:768-770), CMYK converted to RGB (image.rs:23-32), page edges at most 14400 (rules.rs:23).
- **`validate` stays level-agnostic.** It takes no options (lib.rs:176-195), and the fuzz oracle requires that render rejects only what validate rejects (fuzz/src/lib.rs:164-176). So render returns the new code, and the oracle exempts that code for 1b.

**Tests**
- fixtures.rs:118-162 loops over 1b, 2b and 3b, with per-level needles (`%PDF-1.4`, the XMP part, CIDSet).
- A table of fixtures expected to be rejected under 1b, with code and path: boxes-nested, images-jpeg-png and paint-transforms (opacity), images-gif-webp (alpha), svg. All other fixtures must pass `verapdf --flavour 1b` and `--flavour 3b`.
- A new 1b-clean fixture: CFF and TrueType fonts, forms, links, outline, opaque images.
- Unit tests: each transparency source reports its path; each limit error fires.
- Fuzz: pick the level from the input length mod 4. The SVG target asserts that the pre-pass and the writer backstop agree.
- veraPDF runs only locally (ci.sh:28-31). Recommend adding a CI job for it.

**Docs:**
- API.md:41, GUIDE.md:432-433 and 597, README.md:47, MIGRATING.md:30.
- The codes table in scripts/gen-schema-doc.py:79, then regenerate SCHEMA.md.
- packages/inkoxide/index.d.ts:26 and 71.
- TESTING.md:17-21, CHANGELOG, ROADMAP.md:24.

**New dependencies:** none.

**Effort:** M: 3b about half a day; 1b about 4-5 days.

**Risks:**
- CIDSet exactness for CFF subsets (the fonts-cff fixture).
- The SVG pre-pass drifting from the writer.
- Many real documents will be rejected, for example logos that are PNGs with alpha.

**Open decisions**
1. **Partial alpha.** Decided (2026-10-13): reject it with the document path. An opt-in flatten onto white can come later. Binary alpha (typical of GIFs) can become a stencil `/Mask`, which 1b allows: worth a follow-up.
2. **Numbers out of range.** Recommend an error rather than clamping.
3. **Error code name.** Recommend `PDFA_CONFLICT`.
4. **A level-aware `validate`.** Defer.

## Dependencies and order

- The two items share mod.rs, lib.rs, the Node options, `RenderOpts` and build.ts's `output`. Do PDF/A first: the encryption check that forbids PDF/A needs `Profile`.
- **Dependencies on other roadmap items:**
  - File attachments (Later) are what make 3b useful (e.g. e-invoices).
  - Tagged PDF (Later) is needed for the a and u levels.
  - Notes, list boxes and attachments must write their strings and streams through out.rs.
  - Background images, colour emoji, inline images and Canvas gradients must feed the 1b transparency check.
  - Keeping RNG out of core keeps the encryption design compatible with the WebAssembly build.

**Commit sequence** (each one green):
1. `pdf: thread Profile instead of pdf_a` (byte-identical output).
2. `PdfALevel` + 3b across core, Node, render and the CLI.
3. 1b: header, XMP, CIDSet.
4. 1b: reject transparency with the new code, plus the fuzz oracle update.
5. 1b: implementation limits.
6. React `conformance` prop and docs.
7. `pdf/out.rs` + clippy guard (byte-identical output).
8. crypt.rs with `aes`/`sha2` and KATs, plus the core `Encryption` option, the Encrypt dict, the trailer splice and the qpdf tests.
9. Node binding.
10. Render option and React props, with the redaction tests.
11. Fuzz dimensions.
12. Docs, CHANGELOG, ROADMAP.

### Critical Files for Implementation
- crates/core/src/pdf/mod.rs
- crates/core/src/pdf/pdfa.rs
- crates/core/src/pdf/font.rs
- crates/core/src/lib.rs
- crates/core/src/validate/rules.rs
