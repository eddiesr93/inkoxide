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
  | 'STYLE_NOT_APPLICABLE'
  | 'INVALID_TABLE'
  | 'INVALID_NODE'
  | 'UNKNOWN_ANCHOR';

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

/** Mirrors Rust `RenderOptions` (camelCase), plus the per-render inputs. */
export interface RenderOptions {
  /** Input encoding of `doc`. Default `"json"`. */
  format?: 'json' | 'msgpack';
  /** Flate-compress page content streams. Default `true`. */
  compress?: boolean;
  /** Flate level 0–9, used when `compress` is on. Default `6`. */
  compressionLevel?: number;
  /**
   * RFC 3339 timestamp written as the creation date; overrides
   * `meta.creationDate`. With neither set no date is written. Default: none.
   */
  creationDate?: string;
  /** Worker threads for this render. Default: all cores. `1` renders on one thread. */
  threads?: number;
  /** Embed only the glyphs the document uses. Default `true`; `false` embeds whole fonts. */
  subsetFonts?: boolean;
  /**
   * Write PDF/A-2b (archival): XMP metadata, an sRGB output intent and a
   * file identifier. CMYK JPEGs are converted to RGB. Default `false`.
   */
  pdfA?: boolean;
  /** Image bytes (JPEG, PNG, GIF, WebP or SVG) by id, for this document only. */
  images?: Record<string, Uint8Array>;
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

/** Registers (or replaces) a TrueType or CFF OpenType font in the process-wide cache. */
export function registerFont(id: string, data: Uint8Array): void;
/** Removes a registered font; returns whether it existed. */
export function unregisterFont(id: string): boolean;
/**
 * Registers (or replaces), for all later renders, TeX hyphenation patterns
 * for a language: a hyph-utf8 file such as `hyph-en-us.tex` with
 * `\patterns{…}` and optional `\hyphenation{…}` groups, in UTF-8.
 * `lang` is a BCP-47 tag (`"en"`, `"en-GB"`); patterns apply to its
 * language. Registered patterns win over the built-in ones. Throws
 * `HYPHENATION_ERROR` for malformed patterns.
 */
export function registerHyphenationPatterns(lang: string, data: Uint8Array | string): void;
/** Removes registered patterns; returns whether there were any. */
export function unregisterHyphenationPatterns(lang: string): boolean;
/**
 * Validates a document against the schema and the registered fonts. Image
 * references are checked only when `images` is given.
 */
export function validate(
  doc: Uint8Array | string,
  format?: 'json' | 'msgpack',
  images?: Record<string, Uint8Array>,
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
