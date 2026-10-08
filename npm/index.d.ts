// Public contract of the `inkoxide` npm package. Hand-written; keep in
// sync with crates/node/src/lib.rs and npm/index.js.

export interface RenderOptions {
  /** Input encoding of `doc`. Default `"json"`. */
  format?: 'json' | 'msgpack';
  /** Flate-compress page content streams. Default `true`. */
  compress?: boolean;
  /** RFC 3339 timestamp; a fixed value makes output byte-for-byte deterministic. */
  creationDate?: string;
  /** Worker threads for this render. Default: all cores. `1` renders on one thread. */
  threads?: number;
  /** Embed only the glyphs the document uses. Default `true`; `false` embeds whole fonts. */
  subsetFonts?: boolean;
  /** Image bytes (JPEG or PNG) by id, for this document only. */
  images?: Record<string, Uint8Array>;
}

export interface ValidationError {
  /** Stable machine-readable code, e.g. `"UNKNOWN_FONT"`. */
  code: string;
  /** Location in the document, e.g. `"pages[0].children[3].runs[1].font"`. */
  path: string;
  message: string;
}

/** Thrown (or rejected) by every function on failure. */
export class InkoxideError extends Error {
  readonly name: 'InkoxideError';
  readonly code: string;
  /** Empty for errors that are not tied to a document location. */
  readonly path: string;
}

/** Registers (or replaces) a TrueType font in the process-wide cache. */
export function registerFont(id: string, data: Uint8Array): void;
/** Removes a registered font; returns whether it existed. */
export function unregisterFont(id: string): boolean;
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
export function version(): { engine: string; schema: number };
