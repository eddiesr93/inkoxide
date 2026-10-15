# Plan: forms and notes

Part of [Plans for "Next"](README.md). Draft for review; line references point to `f038fed`.

## Forms: list boxes, field groups, formats

**Scope and API**
- React (`packages/react/src/components/index.ts`)
  - `<List name options value multiSelect />`. It is another `field('list','List')` wrapper, like the existing ones (705-717). `value` is a `string`, or a `string[]` when `multiSelect` is set (React DOM does the same for `<select multiple>`).
  - `<FieldSet name="address">…</FieldSet>` is a View, with all View props. Its `name` is put in front of every field name inside it, at any depth: `address.street`, and nested sets give `a.b.c`.
  - `format` on `TextField` takes one of:
    - `{type:'number', decimals?, separators?: '1,234.56'|'1234.56'|'1.234,56'|'1234,56', negative?: 'minus'|'red'|'parens'|'parensRed', currency?, currencyBefore?}`
    - `{type:'percent', decimals?, separators?}`
    - `{type:'date'|'time', pattern}`
    - `{type:'zip'|'zipPlus4'|'phone'|'ssn'}`
  - React also accepts the migration guide's keys (`param`, `nDec`, `sepComma`, `negStyle`, `currencyPrepend`) and its `select` prop for options, and maps them. `Link` already does this kind of aliasing (`src` = `href`, 665-669).
- Schema (`packages/schema/document.v1.json:1550-1640`)
  - `kind` gains `"list"`; new `multiSelect: boolean`; `value` becomes `string | string[]`; `options` applies to select and list.
  - `name` pattern changes from `^[^.]+$` to `^[^.]+(\.[^.]+)*$`: at most 512 characters in total, each segment 1–128, at most 32 segments.
  - `format` is one flat object (`type, decimals, separators, negative, currency, currencyBefore, pattern`). Which keys apply to which `type` is checked in the rules, the way per-kind props are now (`rules.rs:625-664`). A flat struct keeps error paths precise, which is why `RawNode` is flat (`de.rs:601-603`).
- There is no group node: JSON writers use dotted names, which is PDF's own fully qualified name syntax.

**Engine design**
- **Groups**
  - `rules.rs:590-610`: split the name into segments, check each one, and keep a `HashSet` of group prefixes. A name that is both a field and a group (`a` and `a.b`) is `DUPLICATE_ID`.
  - Update the old "`a.b` is invalid" test (`validate/tests.rs:948`) and the `MAX_FIELD_NAME` parity checks (`parity_tests.rs:340-391`).
  - `pdf/forms.rs:29-95`: keep grouping widgets by full name (36-47). For dotted names, create one non-terminal dictionary per prefix, in order of first appearance, with `/T` (the segment), `/Kids` and `/Parent`.
  - A field inside a group gets `.parent(group)` and `/T` = its last segment (140). Only root fields and groups go into `/AcroForm /Fields` (65).
  - Documents without dots get the same object numbers as today, so existing fixtures stay byte-identical.
- **List**
  - Add `FieldKind::List` (`schema/mod.rs:398-405`) and `multi_select`.
  - `value` becomes `Option<FieldValue>` (`One`/`Many`) with a `value_str()` helper, so each existing call site is a one-line change: `forms.rs:107`, `pdf/forms.rs:110,160,171,194`, `rules.rs:668-693`.
  - `forms.rs:54-73`: a list is 150 pt wide like a select; its height is min(option count, 4) rows of size × 1.2, plus 2·`PAD`.
  - `forms.rs:76-160`: paint the visible rows starting at the top index, chosen so the first selected row is visible. Add `FieldOp.highlights: Vec<Rect>`. Include lists in `editable` (99) and in `option_glyphs` (144-149).
  - `field_entries` (148-204): Choice without `COMBO`; `MULTI_SELECT` when set; `/Opt`; `/V` as one string (`choice_value`) or an array (`choice_values`); `/I` with sorted indices; `/TI` when above 0; `/DA` (202).
  - `appearances()` (271-287): fill the highlight rectangles before drawing the text.
- **Formats**
  - New pure module `crates/core/src/forms/format.rs` with three functions:
    - `display(fmt, value) -> {text, color}`: rounds on the decimal string (half away from zero); applies grouping, the negative styles (red sets a colour override), currency, percent ×100, ISO date/time into the pattern with English names, and the zip/phone/SSN masks.
    - `stored(fmt, value)`: what goes in `/V` — the raw number, or the date/time already formatted.
    - `scripts(fmt) -> (keystroke, format)`: the `AFNumber_/AFPercent_/AFDate_…Ex/AFTime_…Ex/AFSpecial_` calls.
  - Validation:
    - `format` only on single-line, non-password text fields, and the value must format.
    - Numbers match `-?\d{1,15}(\.\d{1,15})?`; dates are `YYYY-MM-DD[THH:MM[:SS]]` (add a local-date parser next to `date.rs:17`).
    - `decimals` is 0–10; the pattern is split into tokens checked against a whitelist (`d…dddd m…mmmm yy yyyy H HH h hh M MM s ss t tt`) with the literals ` .,/:-`; `currency` is at most 8 characters, with no control characters, quotes or backslash.
  - Where it is used: the shown text at `forms.rs:106-109`; `/V` at `pdf/forms.rs:160-163`; `/AA` with `/K` and `/F` JavaScript actions in `field_entries`, through pdf-writer's `additional_actions().form_calculate_partial()/form_format()` + `js_string`.
  - **PDF/A**: ISO 19005-2 forbids JavaScript actions and `/AA` on fields and widgets. When `ctx.pdf_a` is set (`pdf/forms.rs:228`), leave the actions out but keep the formatted appearance and `/V`.

**Limits and security**
- **Script injection.** The user's `value` never goes into a script.
  - Numbers in a script come from enums and integers.
  - The only strings (`pattern`, `currency`) are whitelisted in validation and also written through a `js_string_literal` that escapes `\ " '` and writes everything outside printable ASCII as `\uXXXX` (including U+2028/2029).
  - Never concatenate these parameters into a script unescaped.
- New named limits in `rules.rs:124-146`, so the parity tests read them from the schema:
  - field names: 512 characters in total, 32 levels deep;
  - options: at most 1000, each at most 1000 characters (this also tightens select, so it goes under **Security**);
  - `value`: at most 8000 characters. Today `value` and `options` have no bound, so a long value already breaks PDF/A-2's 32,767-byte string limit — that gap exists now.
- `pipeline.rs:374-377`: count the selected values and the scripts in the repeated-content estimate.
- **Fuzzing** (`fuzz/src/lib.rs:867-918`)
  - The generator adds list fields, `multiSelect`, value arrays, dotted names (with bad ones in chaos mode: `a..b`, `.a`, `a` next to `a.b`) and formats whose patterns contain quotes, backslashes and `");app.alert(1);//`.
  - New check in `render_with` (152-175; output is uncompressed): every `/JS` string must match the AF call grammar (a small hand-written checker), and PDF/A output must contain no `/JS` or `/AA`.
  - Extend `document_covers_the_schema` (1255-1270).

**Tests**
- Unit tests:
  - `format.rs`: rounding ties, each negative style, currency before and after, percent, every date token, rejected inputs, escaping.
  - `validate/tests.rs:936`: names, groups, list values, format errors.
  - `pdf/forms.rs`: `/Parent`, `/Kids`, `/T`; `/I` and `/TI` with no `COMBO` flag; `/AA` present normally and absent in PDF/A.
  - `forms.rs`: highlights and formatted text.
- Extend `fixtures/docs/forms.json` and regenerate `forms-1.png` with `scripts/update-expected.sh`. `tests/fixtures.rs` then covers determinism, MessagePack parity, `qpdf --check` and veraPDF 2b.
- Vitest: `react/test/features.test.tsx:945`; `render/test/exports.test.ts:40-122`.
- By hand: Acrobat, pdf.js, Chrome and Preview (Preview ignores JavaScript).

**Docs**
- Regenerate `SCHEMA.md` and `index.d.ts` (`scripts/check-generated.sh`).
- Add entries to `packages/schema/CHANGELOG.md` and `CHANGELOG.md`.
- `GUIDE.md` Forms section (539-552).
- `MIGRATING.md:38`: remove the line and add a forms row. It should say nested sets nest properly and work for every field kind.
- `ROADMAP.md:21`; `API.md:41` (PDF/A leaves form scripts out).

**New dependencies**: none. pdf-writer 0.15 already has `choice_top_index`, `choice_indices`, `choice_values`, `additional_actions`, `js_string` and `MULTI_SELECT`.

**Effort**: field groups S (1.5 days), list M (2.5 days), formats L (4–5 days).

**Risks**
- Viewers differ: Preview ignores JavaScript, and AF function support varies.
- On an exact rounding tie, our appearance and the viewer's reformatted value can differ.
- Month and day names in the appearance are English, whatever the viewer's locale.
- The `FieldValue` type change touches many call sites.
- List text has no shaping, the same limitation select has today.

**Open decisions**
1. Dotted names, or a group node in the schema? *Recommended: dotted names.*
2. `value: string|string[]`, or a separate `values`? *Recommended: the union.*
3. Readable schema keys, with the migration guide's keys accepted as React aliases? *Yes.*
4. PDF/A with `format`: drop the scripts, or fail? *Drop them and document it.* `validate()` never sees the options (`lib.rs:176-207`), and other PDF/A changes are already applied silently (`lib.rs:95-98`).
5. Only `TextField` is exported (`render/src/index.ts:85`), not the migration guide's `TextInput`. *Recommended: exporting a `TextInput` alias and accepting `select`.*
6. `sort`, `edit`, `noSpell`, `align`? *Leave them for later.* `sort` is cheap: sort `/Opt` and set the `SORT` flag.

## Text annotations (`<Note>`)

**Scope and API**
- React: `<Note author? open?>text</Note>`. Children must be strings, as in the migration guide. The colour comes from the `backgroundColor` style or class, which is where the migration guide reads it.
- A note takes no space in the flow. To put it at a position, use `position:'absolute'` with insets. In headers, footers and overlays it repeats on every page.
- Schema: a new node type `{"type":"note","note":{"contents" (1–8000), "color"?, "author"? (≤256), "open"?}}`, built like `field`. It takes `style`, `break`, `pages`, `debug` and `id`, but no link or bookmark (same as fields, `de.rs:595`).

**Engine design**
- **Schema and validation:** add `NodeKind::Note(Box<Note>)` (`schema/mod.rs:341-359`); `NodeType::Note` plus `RawNode.note` (`de.rs:546-599, 606-713`); a `rules.rs:485-525` branch.
- **Layout and paint**
  - `pipeline.rs:83`: measure as zero size; `pipeline.rs:100`: paint through a new `notes.rs`.
  - `notes.rs` produces a `NoteOp`: a 20 pt icon square at the box's top-left, plus contents, colour, author, `open` and the transform in effect.
  - Add `Op::Note` next to `Op::Field` (`paint/mod.rs:124`). `encode_page` collects notes into `EncodedPage.notes`, bounding them under the transform as it does links (`pdf/mod.rs:202-204`). The content stream skips them (911).
- **PDF output** (new `pdf/notes.rs`)
  - Writes `/Subtype /Text`, `/Rect` (`annot_rect`, `mod.rs:567`, moved inside the page), `/Contents`, `/T`, `/Name /Note`, `/C`, `/Open` (a raw dictionary pair), and `/F` = Print|NoZoom|NoRotate.
  - Adds `/AP /N`, a drawn sticky-note icon: a rounded square in the colour, an outline and three lines. PDF/A-2b needs it (6.3.3), and it looks the same in every viewer.
  - Notes are written after links (`mod.rs:320-339`). Object numbers of documents without notes do not change.
- **Pagination:** a note is an atomic zero-height leaf (`paginate/mod.rs:400-403`) and is painted only on the page where it starts (as `forms.rs:92`).
  - Edge case: `starts` leaves out the bottom edge (`paginate/mod.rs:172-174`), and `hides` treats a zero-height box at the window top as hidden (166-168). Test explicitly that a note right after a page break, or exactly at the bottom edge, lands on exactly one page.
- A note inside `Text` waits for "Inline images" (`ROADMAP.md:18`). Shape `NoteOp` as a general icon annotation so "File attachments" can reuse it.
- **React**
  - Add `HOST.Note` to `FLOW` and `TEXT_PARENTS` (`host-config.ts:41-52, 112-120`); any element child gets the error "A <Note> contains only text".
  - `build.ts:641-677`: a new case that joins the text children into `contents`. `checkFixed` (606) allows notes.

**Limits and security**
- Contents at most 8000 characters (PDF/A string limit); author at most 256. pdf-writer's `TextStr` handles escaping. No fonts are needed.
- Add the note size to the estimate at `pipeline.rs:374`.
- Fuzz: add a `note` kind to the generator (`lib.rs:1019-1036`), with control characters and very long strings in chaos mode.

**Tests**
- `pdf/notes.rs`: dictionary entries, flags, that `/AP` is present, and bounds under a transform (as in `mod.rs:1065`).
- Validation tests, and the page-edge test.
- A new `fixtures/docs/notes.json` (in the flow, absolute, in a header) with expected PNGs; `pdftoppm` draws the appearance stream.
- Vitest for the builder and the exports.

**Docs**: SCHEMA, both changelogs, a new GUIDE section, `MIGRATING.md:39`, `ROADMAP.md:22`.

**New dependencies**: none.

**Effort**: M, about 3 days.

**Risks**: `/Open` without a `/Popup` annotation behaves differently across viewers; `gap` in the parent counts the zero-size note like any other child.

**Open decisions**
1. Print flag: always (Acrobat sets /F 28 by default, and PDF/A needs it) or only in PDF/A? *Recommended: always.*
2. An `icon` prop? *Leave it for later*: the appearance stream decides the look, so every icon would need its own drawing.
3. Write a `/Popup` annotation? *No.*
4. Default colour: *yellow `[1, 0.82, 0.2]`*.

## Dependencies and order
- **Inside these items:**
  - Field groups first: it reorganises the field writer and keeps existing output byte-identical.
  - Then the list box, which brings the `FieldValue` change.
  - Then formats, the largest, which edits the same `field_entries`.
  - `<Note>` is independent and can go in parallel.
- **Other roadmap items**
  - **Encryption** (`ROADMAP.md:14`) must encrypt the new strings (`/Contents`, `/T`, `/Opt`, `/V`, `/JS`); whichever lands second adds tests for that.
  - **PDF/A-1b and 3b** (25): decide whether to drop scripts with a conformance check rather than `pdf_a: bool`. PDF/A-1 forbids `/AA` too.
  - **Inline images** (18): makes a note inside `Text` possible.
  - **File attachments** and **tagged PDF** (Later): reuse `NoteOp`; annotations will need node ids.
- **Commit sequence** (each one green):
  1. Limit field options and values (Security entry).
  2. Engine: field tree from dotted names, plus the schema change.
  3. React: `<FieldSet>`.
  4. Engine: `list` kind, `multiSelect`, list appearance stream, fixture.
  5. React: `<List>` and the `select` alias.
  6. Engine: `format.rs`, the formatter on its own, unit-tested.
  7. Engine: `format` on text fields, `/AA` (left out in PDF/A), the fuzz script check.
  8. React: `format` with the migration guide's keys.
  9. Engine: `note` node and its appearance, fixture, fuzz.
  10. React: `<Note>`.
  11. Docs: MIGRATING, ROADMAP, GUIDE.

### Critical Files for Implementation
- crates/core/src/pdf/forms.rs
- crates/core/src/forms.rs
- crates/core/src/validate/rules.rs
- crates/core/src/pdf/mod.rs
- packages/react/src/tree/build.ts
