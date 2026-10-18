# Encryption: a brief for an external cryptography review

Inkoxide is a PDF engine: a Rust core (`crates/core`) that turns a document tree into PDF bytes, with
Node bindings and React/TypeScript packages on top. This brief asks for a review of how it encrypts
PDF files. It describes the construction precisely enough to review without reading the code first,
and then points to the code.

Code described: the commit this file is in. The brief was first written for the tag `v0.9.1`
(`7a08f9a`). Since then the construction has changed in three places: passwords are prepared
with SASLprep, and the optional `entropy` goes into `D` through an HMAC (`encryption.rs`,
`crypt.rs`; see Inputs); tagged output packs objects into object streams and ends with a
cross-reference stream (`pdf/objstm.rs`; see Object streams and the cross-reference stream); and
colour glyphs are drawn by Type 3 fonts (`pdf/type3.rs`; see Font subset tags and widths).
`pdf/mod.rs` was split into modules, and `emit` returns a `Writer`. Code:

- `crates/core/src/pdf/crypt.rs`: the whole derivation, Algorithm 2.B, CBC, HMAC, and the
  unit tests;
- `crates/core/src/pdf/out.rs`: the only writer of strings and streams;
- `crates/core/src/pdf/objstm.rs`: object streams and the cross-reference stream, with
  `Writer::packable` and `Writer::packable_text` in `crates/core/src/pdf/mod.rs`;
- `crates/core/src/pdf/mod.rs`, `PdfWriterSink::write`: the two passes, each running `emit` into a
  `Writer`;
- `crates/core/src/encryption.rs`: options and password checks;
- `crates/core/tests/encryption.rs`: integration tests against qpdf.

## What is standard and what is not

The output is a standard **PDF 2.0 standard security handler, revision 6** file (ISO 32000-2,
7.6.4): `/V 5 /R 6`, `AESV3` crypt filter for strings and streams, `/EncryptMetadata true`, and the
`U`, `UE`, `O`, `OE` and `Perms` entries of Algorithms 2.A, 2.B, 8, 9 and 10. Any conforming reader
opens it; we test with qpdf, pdf.js, PDFium, Preview and Poppler.

What is **not** standard: revision 6 asks for a random 32-byte file key, random 8-byte salts and a
random IV per string and stream. Inkoxide has a hard requirement that **identical input gives
identical output bytes**: the engine has no source of randomness, and users rely on byte-identical
re-renders for caching, diffing and reproducible builds. So every value the standard draws at
random is derived, deterministically, from the passwords and the document. **The derivation is the
subject of this review.**

## Goals

1. **Confidentiality.** Without the user password, nothing about the content of strings and streams
   is learnt beyond what the standard itself leaks (structure, lengths).
2. **No shortcut on password guessing.** Testing a password guess against any public value costs at
   least one Algorithm 2.B evaluation, the same as testing it against `U` or `O`.
3. **No content oracle.** Without the passwords, a guess of the document's content cannot be
   confirmed from the file, except by comparing it with another file made from the same input and
   the same passwords (see Accepted limitations).
4. **Distinct keys.** Two documents that differ anywhere get different file keys, even with the same
   passwords.
5. **Interoperability.** The file is a valid revision 6 file; readers need nothing new.

## Notation

- `‖`: byte concatenation.
- `HMAC(k, m)`: HMAC-SHA-256, RFC 2104.
- `H2B(pw, salt, udata)`: Algorithm 2.B of ISO 32000-2, 7.6.4.3.4, the revision 6 hash. It is SHA-256
  of `pw ‖ salt ‖ udata`, then at least 64 rounds of AES-128-CBC and SHA-256/384/512, keeping the
  first 32 bytes.
- `AES-256-CBC(k, iv, m)`: no padding unless stated.
- `x[a..b]`: bytes `a` to `b-1`.
- `user`, `owner`: the passwords, as bytes.
- Labels are ASCII literals with no terminator (`"key"` is the three bytes `6B 65 79`).

## Inputs

- **Passwords.** Prepared with SASLprep (RFC 4013, as ISO 32000-2, 7.6.4.3.3 asks: mapping, NFKC,
  prohibited characters, bidirectional text, unassigned code points refused) and taken as UTF-8, at
  most 127 bytes each once prepared; a longer one is refused rather than truncated. Printable ASCII
  is unchanged by the preparation.
  - At least one of the two is non-empty.
  - An empty owner password is replaced by the user password, so there is never a file whose owner
    password is empty while the user password is not.
  - Permissions without an owner password, or with one equal to the user password, are refused,
    since the user password would then lift them.
- **P**: the permission bits, a 32-bit integer: bits 1–2 clear, 7–8 and 13–32 set, the others from
  the options.
- **D**: SHA-256 of the complete file that the same render would write **unencrypted**: every
  object, and the cross-reference table and trailer, or the cross-reference stream of a file
  with object streams.
  - The engine writes the file twice: once in plain to compute `D`, then once encrypted. Both
    passes are deterministic and differ only in what `out.rs` does to strings and streams, and in
    the font subset tags and widths below.
  - The plain file is never returned or stored; it is dropped once `D` is computed.
- **E** (optional, the `entropy` option): 16 to 256 bytes the caller draws at random (the JavaScript
  packages draw 32 with `crypto.getRandomValues` for `entropy: true`). With it, `D` is
  `HMAC(E, SHA-256(plain file))` instead, and everything below follows from that `D`; renders of the
  same input then differ, and the determinism limitation below does not apply.

## The derivation

```
-- Salts, unknown without both passwords
S_u   = H2B(user,  HMAC(D, "u")[0..8], "")
S_o   = H2B(owner, HMAC(D, "o")[0..8], "")
salts = HMAC(S_u ‖ S_o, "salts" ‖ D)                    -- 32 bytes
uvs = salts[0..8]   uks = salts[8..16]                  -- user validation and key salts
ovs = salts[16..24] oks = salts[24..32]                 -- owner validation and key salts

-- The standard entries (Algorithms 8 and 9), with these salts
U    = H2B(user, uvs, "") ‖ uvs ‖ uks                   -- 48 bytes
IK_u = H2B(user, uks, "")
O    = H2B(owner, ovs, U) ‖ ovs ‖ oks                   -- 48 bytes
IK_o = H2B(owner, oks, U)

-- Every key from one secret, one label each
M          = IK_u ‖ IK_o                                -- 64 bytes, an HMAC key
file_key   = HMAC(M, "key"   ‖ D)                       -- the AES-256 file key
iv_key     = HMAC(M, "iv"    ‖ D)
perms_tail = HMAC(M, "perms" ‖ D)[0..4]
file_id    = HMAC(M, "id"    ‖ D)[0..16]
tag_key    = HMAC(M, "tag"   ‖ D)

UE    = AES-256-CBC(IK_u, 0^16, file_key)               -- Algorithm 8, step (b)
OE    = AES-256-CBC(IK_o, 0^16, file_key)               -- Algorithm 9, step (b)
Perms = AES-256-ECB(file_key, LE32(P) ‖ FF FF FF FF ‖ "Tadb" ‖ perms_tail)   -- Algorithm 10
/ID   = [file_id, file_id]                               -- in the trailer
```

Labels are joined to the 32-byte `D` without a length prefix. The labels used under one key are
distinct, and `D` has a fixed length, so every input decodes in one way.

### Strings and streams

Every string outside the `Encrypt` dictionary and every stream is sealed, but for the strings
of objects in object streams and the cross-reference stream (next section):

```
seal(place, plain):
    iv  = HMAC(iv_key, place ‖ plain)[0..16]
    out = iv ‖ AES-256-CBC(file_key, iv, PKCS#5-pad(plain))   -- ISO 32000-2, 7.6.3

string number n (n = 1, 2, … in write order):  place = "string" ‖ BE64(n)
stream of object r:                            place = "stream" ‖ BE32(r)
```

- **Unique places.** The string counter is per file, and each stream object is written once, so no
  two seals in a file share a place. The two prefixes differ in their bytes ("string" against
  "stream"), so no place of one kind equals a place of the other.
- **Synthetic IV.** The IV is a PRF of the place and the plaintext, an SIV-style construction over
  CBC, so a repeated IV implies a repeated (place, plaintext) pair.
- **Encryption order.** Streams are compressed (Flate) before encryption.

### Object streams and the cross-reference stream

Tagged output with compression on, other than at PDF/A-1b, packs the structure elements and
the parent tree into object streams (ISO 32000-2, 7.5.7), up to 128 objects each, and ends with
a cross-reference stream (7.5.8) instead of a table.

- The strings of a packed object are written as they are (`Writer::packable_text`): the object
  stream that holds them is sealed as a whole, like any other stream, and 7.6.2 says that
  strings in an object stream are not encrypted on their own.
- The cross-reference stream is not encrypted, as 7.6.2 requires. It holds object numbers,
  offsets and the trailer entries (`/Root`, `/Info`, `/ID`, `/Encrypt`), none of the document's
  strings. `splice_encrypt` adds `/Encrypt` to its dictionary.
- Both passes pack the same objects, so `D` covers the object streams as written unencrypted.

### Font subset tags and widths

Font names (`/BaseFont`, `/FontName`) are PDF names, which are never encrypted. A subset font's
name starts with a six-letter tag. Unencrypted, the tag is made from an xxh3 hash of the
PostScript name and the glyph IDs in the subset. Encrypted, it is:

```
v   = BE64(HMAC(tag_key, postscript_name ‖ 0x00 ‖ plain_tag)[0..8])
tag = six letters 'A' + (v mod 26), v = v / 26 each time, then '+'
```

The widths of a `CIDFont` (`/DW` and `/W`) are numbers, never encrypted either. Unencrypted, `/W`
lists the glyphs used, each CID being the font's own glyph ID. Encrypted, `/W` lists every glyph
of the font, whichever are used: `/DW` is the font's most common width, and `/W` gives the others,
a run of equal widths as one range. The widths then depend on the font alone. This costs up to
about 5 bytes a glyph: some 15 KB for a Latin font of 3,000 glyphs, some 40 KB for a CJK font of
50,000, whose widths are mostly equal.

Colour glyphs (emoji) are drawn by Type 3 fonts, one for each 255 colour glyphs used from a
font. A Type 3 font's `/Widths`, `/FontBBox` and number of glyph procedures are numbers and
names, never encrypted, and describe the colour glyphs used: how many a document draws from
each font, their advances and the box around them stay visible. Their procedures and images
are streams, and are sealed.

## Where secrets go

| Value | Public? | Depends on |
|---|---|---|
| `U`, `O` (with the salts) | yes | passwords, `D` |
| `UE`, `OE`, `Perms`, `/ID`, the IVs | yes | passwords, `D`, content |
| font subset tags | yes | passwords, `D`, glyph set |
| font widths (`/DW`, `/W`) | yes | the font only: every glyph is listed |
| Type 3 font widths, box and glyph count | yes | the colour glyphs used |
| `D`, `S_u`, `S_o`, `IK_u`, `IK_o`, `M`, the keys | no | |

A reader with the user password computes `IK_u`, decrypts `UE`, gets `file_key` and reads
everything. It can usually rebuild the plain file, and so `D`: that is the decrypted file with the
plain subset tags and the widths of the glyphs used only, both of which come from the glyph set
that the decrypted content shows. It still cannot compute `M` or the other keys
without the owner password, unless the two passwords are the same (allowed only without
permissions, when there is nothing for the owner password to guard).

## Accepted limitations (documented in SECURITY.md)

- **Determinism.** Without `entropy`, the same input and passwords give the same bytes. Anyone who
  can make the system encrypt a chosen document with the victim's passwords can confirm a guess of
  another such file by comparing them. With `entropy`, the keys depend on the caller's random bytes
  as well, and this comparison fails.
- **Structure stays visible**, as in any revision 6 file: page count and sizes, object structure,
  annotation rectangles, font names, and the length of every string and stream. Streams are
  compressed before encryption, so their lengths depend on the content: the length of a font
  subset says roughly how many glyphs it holds. A Type 3 font of colour glyphs lists the widths
  of those used.
- **Form field state stays visible** where the standard makes it a name or a number: checkbox
  states (`/V /Yes`, `/AS`), the position of the radio button that is on (radio states are named
  by position when encrypted, with the values in the encrypted `/Opt`), and the positions of a
  list box's selected options (`/I`) and first visible one (`/TI`). Text field values, select and
  list values, and all options are strings, and are encrypted.
- **No integrity.** CBC is malleable and revision 6 has no MAC over the content; `Perms` protects
  only the permission bits.
- **Password strength.** `U` lets anyone test user password guesses at the cost of one `H2B`. Without
  a user password, the file is not confidential at all.
- **Secrets in memory.** Passwords and keys are not zeroised after the render.

## Questions for the reviewer

1. **Salts.** Is `salts = HMAC(H2B(user, …) ‖ H2B(owner, …), "salts" ‖ D)` sound for goals 2 and 3?
   - In particular: with an empty user password, or `owner == user`, can the salts be used as an
     oracle on `D`, or as a cheaper test of the owner password than `O`?
   - Does using an `H2B` output both as an HMAC key and, through `uvs` and `uks`, as salt material
     for further `H2B` calls create any problem?
2. **Key derivation.** Is `HMAC(IK_u ‖ IK_o, label ‖ D)` an adequate KDF here? Two cases:
   - when `IK_u` is known: an empty user password, or a reader who has it;
   - when `IK_u` and `IK_o` are equal: `owner == user` gives different salts, so they should not be
     equal, but we would like this confirmed.
3. **IVs.** Is the synthetic IV `HMAC(iv_key, place ‖ plain)[0..16]` with AES-256-CBC
   deterministic-encryption secure under unique places? `iv_key` and `file_key` are both HMACs of
   the same `M`. Does it matter that a holder of the user password learns `file_key`, but not
   `iv_key`?
4. **D over the plain file.** Is hashing the whole unencrypted file a sound way to bind the keys to
   the content? Does using it as the HMAC key for the pre-salts, `HMAC(D, "u")`, leak anything about
   it?
5. **`/ID` and `Perms`.** Both are derived from `M` and `D`, and both are public; `Perms` is under
   AES-ECB, as the standard requires. Is anything lost compared with the standard's random values?
6. **Subset tags.** Is a keyed tag of about 28 bits enough to stop the glyph set leaking? Note that a
   reader with the user password can decrypt the font programs anyway.
7. **Determinism.** Beyond the equality oracle above, does deterministic output enable attacks that
   the standard's randomness prevents? For example:
   - across documents with shared parts;
   - across re-renders with one field changed.
8. **Anything the standard leaves to the writer** that we got wrong, such as which strings must stay
   unencrypted, `EncryptMetadata`, or the `/ID` rules.

## How to check

- **Unit tests** (`cargo test -p inkoxide crypt`):
  - HMAC against RFC 4231;
  - AES-256-CBC against NIST SP 800-38A;
  - `H2B` against three files encrypted by qpdf 12.4.2: a short password, an empty user password,
    and 127-byte passwords;
  - the salts depend on the passwords;
  - IVs differ by place and by content.
- **Integration tests** (`cargo test -p inkoxide --test encryption`, need qpdf):
  - every fixture decrypts, with either password, to the plain render's objects, but for font
    widths, which give each glyph used its plain width;
  - no plaintext string is in plain sight;
  - no plain subset tag survives;
  - font widths are the same whatever the text;
  - changing an image, link, field, note or bookmark changes the key and `/ID`;
  - bad options never echo a password.
- **By hand:** the integration tests leave each fixture encrypted in `target/fixtures-encrypted/`
  (user password `user (pw) \ 1`, owner password `owner-pw`), next to its plain render.
  ```sh
  qpdf --show-encryption --password='owner-pw' target/fixtures-encrypted/text-v1-1.pdf
  qpdf --qdf --object-streams=disable --password='owner-pw' target/fixtures-encrypted/text-v1-1.pdf /tmp/decrypted.pdf
  ```

Dependencies: RustCrypto `aes` 0.9.3 (AES-NI or ARMv8 AES where the CPU has it, otherwise its constant-time fixsliced software path) and `sha2` 0.11.1, and `stringprep` 0.1.5 for SASLprep; HMAC,
CBC and `H2B` are implemented in `crypt.rs` (about 80 lines).
