// Public contract of the `@inkoxide/wasm` npm package: the contract of the
// `inkoxide` package (packages/inkoxide/index.d.ts), plus `init` and
// `clearCaches`. Hand-written; keep in sync with crates/wasm/src/lib.rs,
// index.js and the `inkoxide` package.

/**
 * Where the engine's WebAssembly module comes from: a URL (by default
 * `inkoxide.wasm` next to this module), a `Response`, its bytes, or the
 * module compiled already.
 */
export type WasmSource = URL | string | Response | PromiseLike<Response> | BufferSource | WebAssembly.Module;

export interface InitOptions {
  /**
   * The folder of the built-in hyphenation patterns (`<file>.bin`), fetched as documents need
   * them. Default: the package's `hyphenation/`, which bundlers copy.
   */
  hyphenation?: string | URL;
}

/**
 * Compiles and instantiates the engine. Every other function throws until
 * the promise resolves. Later calls return the same promise and ignore
 * their arguments.
 */
export function init(source?: WasmSource, options?: InitOptions): Promise<void>;

/**
 * Fetches the built-in hyphenation patterns for `langs` (BCP-47 tags) that are not loaded,
 * for `renderSync`; `render` fetches those a document needs itself. The module does not embed
 * them: each language is a download of its own.
 */
export function loadHyphenation(langs: string[]): Promise<void>;

/**
 * Empties the caches renders share (decoded images, font subsets, SVG laid
 * out with its text). The memory stays with the instance, for later
 * renders: WebAssembly memory does not shrink.
 */
export function clearCaches(): void;

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
 * (mirrors Rust `InkoxideError::code`), a JS argument of the wrong type
 * or out of range (`INVALID_ARGUMENT`), or a bug inside
 * the engine (`INTERNAL`). After `INTERNAL`, the next call runs in a new
 * instance of the engine, with the fonts and patterns registered so far.
 */
export type ErrorCode =
  | ValidationCode
  | 'FONT_ERROR'
  | 'IMAGE_ERROR'
  | 'LAYOUT_ERROR'
  | 'PDF_ERROR'
  | 'HYPHENATION_ERROR'
  | 'INVALID_ARGUMENT'
  | 'NOT_LOADED'
  | 'INTERNAL';

/** A PDF/A part and conformance level (mirrors Rust `PdfALevel`). */
export type PdfALevel = '1b' | '2b' | '2a' | '3b' | '3a';

/**
 * Mirrors Rust `RenderOptions` (camelCase), plus the per-render inputs. Leave an option out with
 * `undefined`; `null` or a value of another type is `INVALID_ARGUMENT` at its path
 * (`options.threads`), without the value in the message.
 */
export interface RenderOptions {
  /**
   * Input encoding of `doc`. Default `"json"`. This build reads JSON only:
   * `"msgpack"` is a `PARSE_ERROR`.
   */
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
  /** Threads for this render: only `1`. WebAssembly renders on the calling thread. */
  threads?: number;
  /** Embed only the glyphs the document uses. Default `true`; `false` embeds whole fonts. */
  subsetFonts?: boolean;
  /**
   * Write PDF/A (archival): XMP metadata, an sRGB output intent and a
   * file identifier. CMYK JPEGs are converted to RGB. `true` writes
   * PDF/A-2b; `'3b'` writes PDF/A-3b, which may carry embedded files;
   * `'1b'` writes PDF/A-1b; `'2a'` and `'3a'` are 2b and 3b, tagged.
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
   * title, a language, `alt` on its images or `label` on its fields fails with `UA_CONFLICT`.
   * Default `false`.
   */
  pdfUA?: boolean;
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
  /**
   * Fonts the document declares that are fetched only if the render needs them, by id: the URL
   * of each. A render that does not draw with one, or try it for a character (a fallback),
   * renders as if it were registered; `render` fetches and registers those it needs, and
   * `renderSync` fails with `NOT_LOADED`.
   */
  deferredFonts?: Record<string, string | URL>;
}

/** Mirrors Rust `Encryption`. */
export interface EncryptionOptions {
  /**
   * The password that opens the file. Printable ASCII, at most 127 bytes.
   * Default: none (anyone opens it).
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

/**
 * Registers (or replaces) a TrueType, CFF OpenType, WOFF or WOFF2 font for all later renders. A
 * WOFF2 font gives other PDF bytes than its TrueType file; WOFF gives the same.
 */
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
 * references (`src`) are checked only when `images` is given: each must be
 * there and decode. Inline images (`data`) are always checked. Returns at
 * most 100 problems; one in the document's structure is returned alone. An
 * empty array means the document is valid; `render` can still fail on an
 * option, the page limit, the limit on content repeated on every page or a
 * font that cannot be embedded.
 */
export function validate(
  doc: Uint8Array | string,
  format?: 'json' | 'msgpack',
  images?: Record<string, Uint8Array>,
): ValidationError[];
/**
 * Renders to PDF, after fetching what the document needs: built-in hyphenation patterns and its
 * `deferredFonts`. The render itself runs on the calling thread, so in a page call it from a worker.
 */
export function render(doc: Uint8Array | string, options?: RenderOptions): Promise<Uint8Array>;
/**
 * Renders on the calling thread, with what is loaded: a document that needs built-in
 * hyphenation patterns not loaded (`loadHyphenation`) or one of its `deferredFonts` fails with
 * `NOT_LOADED`.
 */
export function renderSync(doc: Uint8Array | string, options?: RenderOptions): Uint8Array;
export interface Version {
  /** Engine version, e.g. `"0.1.0"`. */
  engine: string;
  /** The document `version` this engine reads. */
  schema: number;
  /** Commit the module was built from (12 hex digits), or `"unknown"`. */
  gitSha: string;
}
export function version(): Version;
