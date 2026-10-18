// Public contract of the `inkoxide` npm package. Hand-written; keep in
// sync with crates/node/src/lib.rs and index.js.

/**
 * Codes of document problems, from `validate` and from a failed `render`
 * (mirrors Rust `ValidationCode`). New codes may be added in minor
 * versions; handle unknown strings.
 */
export type ValidationCode =
  | 'PARSE_ERROR'
  | 'UNSUPPORTED_VERSION'
  | 'UNKNOWN_FIELD'
  | 'MISSING_FIELD'
  | 'INVALID_TYPE'
  | 'INVALID_VALUE'
  | 'OUT_OF_RANGE'
  | 'DUPLICATE_ID'
  | 'UNKNOWN_FONT'
  | 'FONT_NOT_REGISTERED'
  | 'UNKNOWN_IMAGE'
  | 'INVALID_IMAGE'
  | 'IMAGE_TOO_LARGE'
  | 'UNKNOWN_ATTACHMENT'
  | 'ATTACHMENT_NOT_REGISTERED'
  | 'ATTACHMENT_TOO_LARGE'
  | 'STYLE_NOT_APPLICABLE'
  | 'INVALID_TABLE'
  | 'INVALID_NODE'
  | 'UNKNOWN_ANCHOR'
  | 'PDFA_CONFLICT'
  | 'UA_CONFLICT';

/**
 * Every `InkoxideError.code`: a document problem, a resource failure
 * (mirrors Rust `InkoxideError::code`), a JS argument napi could not
 * convert or that is out of range (`INVALID_ARGUMENT`), or a bug inside
 * the engine (`INTERNAL`; the process keeps running).
 */
export type ErrorCode =
  | ValidationCode
  | 'FONT_ERROR'
  | 'IMAGE_ERROR'
  | 'LAYOUT_ERROR'
  | 'PDF_ERROR'
  | 'HYPHENATION_ERROR'
  | 'INVALID_ARGUMENT'
  | 'INTERNAL';

/** A PDF/A part and conformance level (mirrors Rust `PdfALevel`). */
export type PdfALevel = '1b' | '2b' | '2a' | '3b' | '3a';

/**
 * Mirrors Rust `RenderOptions` (camelCase), plus the per-render inputs. Leave an option out with
 * `undefined`; `null` or a value of another type is `INVALID_ARGUMENT` at its path
 * (`options.threads`), without the value in the message.
 */
export interface RenderOptions {
  /** Input encoding of `doc`. Default `"json"`. */
  format?: 'json' | 'msgpack';
  /** Flate-compress page content streams. Default `true`. */
  compress?: boolean;
  /** Flate level, an integer from 0 to 9, used when `compress` is on. Default `6`. */
  compressionLevel?: number;
  /**
   * RFC 3339 timestamp written as the creation date; overrides
   * `meta.creationDate`. With neither set no date is written. Default: none.
   * Another string is `INVALID_ARGUMENT`.
   */
  creationDate?: string;
  /**
   * Worker threads for this render, an integer from 1 to 256. Default: all cores. `1` renders
   * on the libuv thread that runs it, with no parallelism inside, so concurrent renders do not
   * wait for each other. Any other number uses a pool of that size, kept for the life of the
   * process and shared by the renders that ask for the same number.
   */
  threads?: number;
  /** Embed only the glyphs the document uses. Default `true`; `false` embeds whole fonts. */
  subsetFonts?: boolean;
  /**
   * Write PDF/A (archival): XMP metadata, an sRGB output intent and a
   * file identifier. CMYK JPEGs are converted to RGB. `true` writes
   * PDF/A-2b; `'3b'` writes PDF/A-3b, which may carry embedded files;
   * `'1b'` writes PDF/A-1b; `'2a'` and `'3a'` write PDF/A-2a and 3a: 2b and 3b, tagged.
   * Another string is `INVALID_ARGUMENT`.
   * Default `false`.
   */
  pdfA?: boolean | PdfALevel;
  /**
   * Write tagged PDF: a structure tree with the reading order (document order) and what each
   * part is (headings, paragraphs, figures with their `alt`, tables, lists), and the rest marked
   * as decoration, for screen readers and reflow. `pdfUA` and the levels `'2a'` and `'3a'` imply
   * it. Default `false`.
   */
  tagged?: boolean;
  /**
   * Write PDF/UA-1 (ISO 14289-1): tagged, with the metadata it asks for. A document without a
   * title, a language, `alt` on its images and figures or `label` on its fields, with headings
   * that skip a level, or with a `listItem` outside a `list` (a `listBody` outside a
   * `listItem`), fails with `UA_CONFLICT`. Default `false`.
   */
  pdfUA?: boolean;
  /**
   * At PDF/A-1b, which forbids transparency, draw images whose alpha is partial (neither only
   * clear nor only opaque) on white instead of failing with `PDFA_CONFLICT`. Other levels keep
   * the alpha. Binary alpha, such as a GIF's, needs no option: PDF/A-1b writes it as a stencil
   * mask. Default `false`.
   */
  flattenAlpha?: boolean;
  /** Image bytes (JPEG, PNG, GIF, WebP or SVG) by id, for this document only. */
  images?: Record<string, Uint8Array>;
  /**
   * The bytes of the files the document's `attachments` name in `src`, by id. A `src`
   * missing from here is `ATTACHMENT_NOT_REGISTERED`.
   */
  attachments?: Record<string, Uint8Array>;
  /**
   * Encrypt the PDF with AES-256 (PDF 2.0). Not with `pdfA`. Default: not
   * encrypted. A bad value is `INVALID_ARGUMENT` at its path, without the
   * password in the message.
   */
  encryption?: EncryptionOptions;
}

/** Mirrors Rust `Encryption`. */
export interface EncryptionOptions {
  /**
   * The password that opens the file: text SASLprep (RFC 4013) accepts,
   * at most 127 bytes of UTF-8 once prepared. Default: none (anyone opens it).
   */
  userPassword?: string;
  /**
   * The password that lifts the permissions. Default: the user password,
   * so whoever opens the file may do anything. At least one of the two is
   * required.
   */
  ownerPassword?: string;
  /** What a reader without the owner password may do. Needs an `ownerPassword` other than `userPassword`. */
  permissions?: Permissions;
  /**
   * Random bytes (16 to 256) mixed into the keys, so each render of the same
   * document gets other keys; `true`: 32 from `crypto.getRandomValues`. Default:
   * none, and the same document and passwords give the same file.
   */
  entropy?: boolean | Uint8Array;
}

/**
 * Mirrors Rust `Permissions`. Every permission is off unless set, except
 * `contentAccessibility`, which is on. Readers enforce them.
 */
export interface Permissions {
  printing?: 'lowResolution' | 'highResolution';
  modifying?: boolean;
  copying?: boolean;
  annotating?: boolean;
  fillingForms?: boolean;
  contentAccessibility?: boolean;
  documentAssembly?: boolean;
}

export interface ValidationError {
  /** Stable machine-readable code, e.g. `"UNKNOWN_FONT"`. */
  code: ValidationCode;
  /** Location in the document, e.g. `"pages[0].children[3].runs[1].font"`. */
  path: string;
  /** Human-readable description. */
  message: string;
}

/** Thrown (or rejected) by every function on failure. */
export class InkoxideError extends Error {
  readonly name: 'InkoxideError';
  readonly code: ErrorCode;
  /** Empty for errors that are not tied to a document location. */
  readonly path: string;
}

/** Registers (or replaces) a TrueType, CFF OpenType, WOFF or WOFF2 font in the process-wide cache. */
export function registerFont(id: string, data: Uint8Array): void;
/** Removes a registered font; returns whether it existed. */
export function unregisterFont(id: string): boolean;
/**
 * Registers (or replaces), for all later renders, TeX hyphenation patterns
 * for a language: a hyph-utf8 file such as `hyph-en-us.tex` with
 * `\patterns{…}` and optional `\hyphenation{…}` groups, in UTF-8.
 * `lang` is a BCP-47 tag with a two- or three-letter language (`"en"`,
 * `"en-GB"`, `"haw"`). Text uses the patterns of its own tag, else of the
 * nearest less specific one (`"en-GB"`, then `"en"`), else the built-in
 * ones. Throws `HYPHENATION_ERROR` for a malformed tag or patterns.
 */
export function registerHyphenationPatterns(lang: string, data: Uint8Array | string): void;
/** Removes the patterns registered for exactly `lang`; returns whether there were any. */
export function unregisterHyphenationPatterns(lang: string): boolean;
/**
 * Validates a document against the schema and the registered fonts. Image
 * references (`src`) are checked only when `images` (or `options.images`) is
 * given: each must be there and decode. Inline images (`data`) are always
 * checked. Returns at most 100 problems; one in the document's structure is
 * returned alone. An empty array means the document is valid.
 *
 * With `options`, those a render will be given (`format` and `images` win
 * over theirs), it also reports what that render would refuse: what the PDF/A
 * level or PDF/UA does not allow (`PDFA_CONFLICT`, `UA_CONFLICT`), an
 * attachment's `src` missing from `options.attachments` or too large, and an
 * invalid `creationDate` or `encryption` (`INVALID_VALUE` at `options.…`). A
 * wrong type or value of another option throws, as `render` does. Without
 * them, `render` can also fail on those. Either way it can fail on the page
 * limit, the limit on content repeated on every page, a font that cannot be
 * embedded, or at PDF/A-1b a colour glyph that needs transparency.
 */
export function validate(
  doc: Uint8Array | string,
  format?: 'json' | 'msgpack',
  images?: Record<string, Uint8Array>,
  options?: RenderOptions,
): ValidationError[];
/** Renders to PDF on the libuv thread pool; never blocks the event loop. */
export function render(doc: Uint8Array | string, options?: RenderOptions): Promise<Uint8Array>;
/** Renders on the calling thread (CLIs, tests). */
export function renderSync(doc: Uint8Array | string, options?: RenderOptions): Uint8Array;
export interface Version {
  /** Engine version, e.g. `"0.1.0"`. */
  engine: string;
  /** The document `version` this engine reads. */
  schema: number;
  /** Commit the native binary was built from (12 hex digits), or `"unknown"`. */
  gitSha: string;
}
export function version(): Version;
