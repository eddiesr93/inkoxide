import type { Document } from '@inkoxide/schema';

export interface RenderOpts {
  /** RFC 3339 timestamp written to the PDF; omit for deterministic output. */
  creationDate?: string;
  /** Engine worker threads for this render (default: all cores). */
  threads?: number;
  /** Flate-compress content streams (default true). */
  compress?: boolean;
  /** Embed only the glyphs used (default true). */
  subsetFonts?: boolean;
  /** Write PDF/A-2b (default false). */
  pdfA?: boolean;
  /**
   * Extra image bytes by id, for `<Image src="id">`. An id wins over a file path or URL with the
   * same text.
   */
  images?: Record<string, Uint8Array>;
  /**
   * Remote images `<Image src="https://…">` and `<Image src={{ uri }}>` may use. Defaults to the
   * comma-separated `INKOXIDE_REMOTE_IMAGES` environment variable; without either, remote URLs
   * are rejected.
   */
  remoteImages?: RemoteImageOptions;
  /**
   * Folders `<Image src="./logo.png">` may read files from. Defaults to the
   * `INKOXIDE_LOCAL_IMAGES` environment variable (folders separated by `:`, or `;` on Windows);
   * without either, no file is read and a path in `src` is an image id like any other string.
   */
  localImages?: LocalImageOptions;
  /** How long a function or promise `<Image src>` may take to settle (default 10 000 ms). */
  imageTimeoutMs?: number;
}

export interface LocalImageOptions {
  /**
   * The folders image files may come from. A relative path (`./logo.png`) is resolved against
   * the first; any path must lead, symbolic links followed, to a file inside one of them.
   */
  root: string | readonly string[];
  /** Largest accepted file in bytes (default 20 MiB). */
  maxBytes?: number;
}

export interface RemoteImageOptions {
  /** URL prefixes (`'https://cdn.example.com/img/'`) or host names (`'*.example.com'`, https only). */
  allow: readonly string[];
  /** Per-image timeout for the whole download (default 10 000 ms; `Infinity` for none). */
  timeoutMs?: number;
  /** Largest accepted image in bytes (default 20 MiB). */
  maxBytes?: number;
}

export interface Timings {
  /** React render to host instances. */
  reactMs: number;
  /** Instances to the schema tree, Tailwind resolution included. */
  treeMs: number;
  /** ajv validation (dev only; 0 in production). */
  validateMs: number;
  /** The engine render (every pass, with `render` props). */
  engineMs: number;
  /** Loading images: remote URLs and requests, files, functions and promises. */
  fetchMs: number;
  /** Everything, data validation included. */
  totalMs: number;
}

export interface RenderResult {
  pdf: Uint8Array;
  timings: Timings;
  pages: number;
  /** Engine renders: 1, or more when `render` props needed the page count. */
  passes: number;
  bytes: number;
  schemaVersion: number;
  engineVersion: string;
  /** Font ids the document uses. */
  fonts: string[];
  /** Non-fatal problems (font fallbacks, missing hyphenation patterns). */
  warnings: string[];
  /** The document tree that was rendered. */
  document: Document;
}

export type RenderErrorCode =
  /** The template id does not exist (404). */
  | 'UNKNOWN_TEMPLATE'
  /** The data does not match the template's zod schema (400). */
  | 'INVALID_DATA'
  /** The template threw, or uses components or classes wrongly (400). */
  | 'TEMPLATE_ERROR'
  /** The built tree does not match the engine schema (400). */
  | 'INVALID_TREE'
  /** The engine rejected the document: fonts, images, tables, layout (400). */
  | 'ENGINE_REJECTED'
  /** A font added with `Font.register` could not be read, fetched or parsed (400). */
  | 'INVALID_FONT'
  /** An allowed remote image could not be fetched (502). */
  | 'IMAGE_FETCH_FAILED'
  /** An image file could not be read (400). */
  | 'IMAGE_READ_FAILED'
  /**
   * The engine could not start (no fonts, mismatched versions), `renderToFile` could not write,
   * or a bug: an INTERNAL or INVALID_ARGUMENT engine error, or anything unexpected (500).
   */
  | 'INTERNAL';

export interface DataIssue {
  path: string;
  message: string;
  code: string;
}

export interface RenderErrorInit {
  status?: number;
  path?: string;
  component?: string;
  componentStack?: string;
  className?: string;
  engineCode?: string;
  issues?: DataIssue[];
  cause?: unknown;
}

/** An error a template author can act on as is: what went wrong, and in which component. */
export class RenderError extends Error {
  override readonly name = 'RenderError';
  readonly code: RenderErrorCode;
  readonly status: number;
  readonly path: string | undefined;
  readonly component: string | undefined;
  readonly componentStack: string | undefined;
  readonly className: string | undefined;
  readonly engineCode: string | undefined;
  readonly issues: DataIssue[] | undefined;

  constructor(code: RenderErrorCode, message: string, init: RenderErrorInit = {}) {
    super(message, init.cause === undefined ? undefined : { cause: init.cause });
    this.code = code;
    this.status =
      init.status ??
      (code === 'UNKNOWN_TEMPLATE'
        ? 404
        : code === 'INTERNAL'
          ? 500
          : code === 'IMAGE_FETCH_FAILED'
            ? 502
            : 400);
    this.path = init.path;
    this.component = init.component;
    this.componentStack = init.componentStack;
    this.className = init.className;
    this.engineCode = init.engineCode;
    this.issues = init.issues;
  }

  toJSON(): Record<string, unknown> {
    const out: Record<string, unknown> = { code: this.code, message: this.message };
    for (const k of ['path', 'component', 'componentStack', 'className', 'engineCode', 'issues'] as const) {
      if (this[k] !== undefined) out[k] = this[k];
    }
    return out;
  }
}
