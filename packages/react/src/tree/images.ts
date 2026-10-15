/**
 * Image `src`s: what each kind of `src` is, which URLs a template may use, and the ids a tree
 * gives the images the caller loads. `@inkoxide/render` reads the values of function and promise
 * `src`s with the same rules.
 */
import type { ImageRequest, ImageSrcValue } from '../components/index.ts';

/** Which remote image URLs `<Image src>` may use; the caller fetches them (`@inkoxide/render` does). */
export interface RemoteImagePolicy {
  /**
   * Allowed URLs (`'https://cdn.example.com/img/'`: the same origin, and the entry's path or a
   * path under it) or host names, with `*.` for subdomains (`'*.example.com'`; host entries
   * allow `https:` only). Ports must match: an entry without a port allows only its scheme's
   * default port (443 for `https:`, 80 for `http:`), and `'cdn.example.com:8443'` or
   * `'https://cdn.example.com:8443'` only port 8443.
   */
  allow: readonly string[];
}

/** True when `url` is allowed by the policy. */
export function remoteAllowed(url: string, policy: RemoteImagePolicy | undefined): boolean {
  if (!policy) return false;
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return false;
  }
  if (u.username || u.password) return false;
  return policy.allow.some((entry) => {
    if (entry.includes('://')) return urlEntryAllows(entry, u);
    if (u.protocol !== 'https:') return false;
    const [, e = '', port = '443'] = /^(.*?)(?::(\d+))?$/.exec(entry.toLowerCase()) ?? [];
    if ((u.port || '443') !== String(Number(port))) return false;
    const host = u.hostname.toLowerCase();
    return e.startsWith('*.') ? host.endsWith(e.slice(1)) && host.length > e.length - 1 : host === e;
  });
}

/**
 * Whether a URL entry allows `u`: the same origin (scheme, host and port), and, when the entry
 * has a path, that path or one under it (`/img` allows `/img` and `/img/a.png`, not `/imgs`).
 */
function urlEntryAllows(entry: string, u: URL): boolean {
  let e: URL;
  try {
    e = new URL(entry);
  } catch {
    return false;
  }
  if ((u.protocol !== 'https:' && u.protocol !== 'http:') || e.origin !== u.origin) return false;
  const path = e.pathname;
  return path === '/' || u.pathname === path || u.pathname.startsWith(path.endsWith('/') ? path : `${path}/`);
}

/**
 * The message for a URL the remote image policy does not allow; `prop` is the `src` as written
 * (`src="https://…"`), and `elsewhere` another place the caller reads the allowlist from.
 */
export function remoteRefused(prop: string, elsewhere?: string): string {
  const option = elsewhere === undefined ? 'remoteImages.allow' : `remoteImages.allow (or ${elsewhere})`;
  return `<Image ${prop}>: this URL is not in the remoteImages allowlist. Add its origin to the option ${option}, or pass the bytes.`;
}

/**
 * True when an image `src` string is written as a file path: absolute (`/img/logo.png`,
 * `C:\img\logo.png`), relative with `./` or `../`, or a `file:` URL. Other strings are image ids,
 * so a template never reads a file by accident. Paths are files only when the caller allows them
 * (`localImages`); otherwise they are ids too.
 */
export function isImagePath(src: string): boolean {
  return /^(?:\.{0,2}[\\/]|[a-z]:[\\/]|file:)/i.test(src);
}

/**
 * What an image `src` is. A string is, in this order: the id of an image the caller passes in
 * (`registered`), a `data:` URL, a file path, a URL with a scheme (`https://…`), else an id that
 * names no image yet (`''` included). An object that is not bytes or a promise is a `request`,
 * its `uri` not checked.
 */
export type ImageSource =
  | { kind: 'bytes'; bytes: Uint8Array | ArrayBuffer }
  | { kind: 'registered' | 'data' | 'path' | 'url' | 'id'; src: string }
  | { kind: 'function'; src: () => ImageSrcValue | Promise<ImageSrcValue> }
  | { kind: 'promise'; src: PromiseLike<ImageSrcValue> }
  | { kind: 'request'; request: ImageRequest }
  | { kind: 'none' };

/** Sorts an image `src` (see `ImageSource`); `registered` says whether a string is a caller's id. */
export function imageSource(src: unknown, registered: (id: string) => boolean): ImageSource {
  if (src instanceof Uint8Array || src instanceof ArrayBuffer) return { kind: 'bytes', bytes: src };
  if (typeof src === 'string') {
    if (registered(src)) return { kind: 'registered', src };
    if (src.startsWith('data:')) return { kind: 'data', src };
    if (isImagePath(src)) return { kind: 'path', src };
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(src)) return { kind: 'url', src };
    return { kind: 'id', src };
  }
  if (typeof src === 'function') {
    return { kind: 'function', src: src as () => ImageSrcValue | Promise<ImageSrcValue> };
  }
  if (isThenable(src)) return { kind: 'promise', src };
  if (typeof src === 'object' && src !== null) return { kind: 'request', request: src as ImageRequest };
  return { kind: 'none' };
}

const isThenable = (v: unknown): v is PromiseLike<ImageSrcValue> =>
  typeof (v as PromiseLike<unknown> | null)?.then === 'function';

/**
 * An image the caller loads before rendering (`@inkoxide/render` does): a file, a request
 * allowed by the remote image policy, or a function or promise `src`, not yet called or awaited.
 */
export type PendingImage =
  | { kind: 'file'; path: string }
  | { kind: 'request'; request: ImageRequest }
  | { kind: 'deferred'; src: Promise<ImageSrcValue> | (() => ImageSrcValue | Promise<ImageSrcValue>) };

/**
 * The ids a tree gives the images it cannot refer to by the caller's id: one per bytes, file path,
 * URL, request, function or promise. A tree and its rebuilds share one table, so an image keeps
 * its id on every pass and an id never names two images. The caller's ids are skipped, so a
 * generated id never names one of their images.
 */
export class ImageIds {
  private readonly ids = new Map<unknown, string>();
  private readonly counts = new Map<string, number>();

  constructor(private readonly taken: ReadonlySet<string> = new Set()) {}

  /** The id of `key` (a string, or an object by identity); a new one is `${prefix}-${n}`. */
  of(key: unknown, prefix: string): string {
    let id = this.ids.get(key);
    if (id === undefined) {
      let n = this.counts.get(prefix) ?? 0;
      while (this.taken.has(`${prefix}-${n}`)) n++;
      this.counts.set(prefix, n + 1);
      id = `${prefix}-${n}`;
      this.ids.set(key, id);
    }
    return id;
  }
}

export interface ImageTableOptions {
  /** Ids of the images the caller passes in: a `src` string naming one is that image, path or not. */
  registered: ReadonlySet<string>;
  /** The id table shared with the tree's rebuilds. */
  generated: ImageIds;
  remoteImages: RemoteImagePolicy | undefined;
  /** Where else the caller reads the allowlist from, for `remoteRefused`. */
  remoteElsewhere?: string | undefined;
  /** File paths in `src` are images to load; otherwise they are ids. */
  localImages: boolean | undefined;
}

/**
 * The images of one build: what each `<Image src>` becomes in the image node, and the images
 * the caller passes to the engine (`images`) or loads first (`remote`, `pending`).
 */
export class ImageTable {
  /** Image bytes referenced by generated `src` ids. */
  readonly images: Record<string, Uint8Array> = {};
  /** Remote images to fetch: image id → URL. */
  readonly remote: Record<string, string> = {};
  /** Other images to load: image id → a file, a request, or a function or promise `src`. */
  readonly pending: Record<string, PendingImage> = {};

  constructor(private readonly opts: ImageTableOptions) {}

  /**
   * The image node's `src` (an image id) or `data` for `src`. `fail` reports a `src` that is
   * missing, a request without a uri, or a URL the remote image policy does not allow; messages
   * name the element as `component`.
   */
  resolve(
    src: unknown,
    fail: (message: string) => never,
    component = 'Image',
  ): { src: string } | { data: string } {
    const s = imageSource(src, (id) => this.opts.registered.has(id));
    switch (s.kind) {
      case 'bytes': {
        const id = this.opts.generated.of(s.bytes, 'image');
        this.images[id] = bytesOf(s.bytes);
        return { src: id };
      }
      case 'registered':
        return { src: s.src };
      case 'data':
        return { data: s.src };
      case 'path':
        // Without localImages a path, `file:` URLs included, is an image id like any other string.
        if (!this.opts.localImages) return { src: s.src };
        return { src: this.pendingId(`file\0${s.src}`, 'file', () => ({ kind: 'file', path: s.src })) };
      case 'url': {
        if (!remoteAllowed(s.src, this.opts.remoteImages)) {
          fail(remoteRefused(`src="${s.src}"`, this.opts.remoteElsewhere));
        }
        const id = this.opts.generated.of(`remote\0${s.src}`, 'remote');
        this.remote[id] = s.src;
        return { src: id };
      }
      case 'id':
        if (s.src.length > 0) return { src: s.src };
        break;
      case 'function':
        return { src: this.pendingId(s.src, 'deferred', () => ({ kind: 'deferred', src: s.src })) };
      case 'promise':
        return {
          src: this.pendingId(s.src, 'deferred', () => {
            // Handled now, awaited by the caller: a rejection before then must not crash the process.
            const promise = Promise.resolve(s.src);
            promise.catch(() => {});
            return { kind: 'deferred', src: promise };
          }),
        };
      case 'request': {
        const { request } = s;
        if (typeof request.uri !== 'string' || request.uri === '') {
          fail(`<${component} src={{ uri }}>: a request needs a uri.`);
        }
        if (!remoteAllowed(request.uri, this.opts.remoteImages)) {
          fail(remoteRefused(`src={{ uri: "${request.uri}" }}`, this.opts.remoteElsewhere));
        }
        const key = requestKey(request);
        return {
          src: this.pendingId(key === undefined ? request : `request\0${key}`, 'request', () => ({
            kind: 'request',
            request,
          })),
        };
      }
    }
    return fail(
      `<${component}> needs a src: bytes, a data: URL, an image id, a file path, a URL, a request, or a function or promise.`,
    );
  }

  /** One id per file path, request, function or promise, however many images use it. */
  private pendingId(key: unknown, prefix: string, image: () => PendingImage): string {
    const id = this.opts.generated.of(key, prefix);
    this.pending[id] ??= image();
    return id;
  }
}

/**
 * What a request sends, as a string: requests that send the same thing share an id however often
 * a template creates them. Undefined for a body that is neither a string nor bytes.
 */
function requestKey(r: ImageRequest): string | undefined {
  const { body } = r;
  // Only plain header objects are compared; a `Headers` instance keeps its request apart.
  const proto = r.headers === undefined ? null : Object.getPrototypeOf(r.headers);
  if (proto !== null && proto !== Object.prototype) return undefined;
  const bytes = body instanceof Uint8Array;
  if (body !== undefined && typeof body !== 'string' && !bytes) return undefined;
  const sent = bytes ? Array.from(body, (b) => b.toString(16).padStart(2, '0')).join('') : body;
  const headers = Object.entries(r.headers ?? {})
    .map(([k, v]) => [k.toLowerCase(), String(v)])
    .sort(([a = ''], [b = '']) => (a < b ? -1 : a > b ? 1 : 0));
  return JSON.stringify([r.uri, (r.method ?? 'GET').toUpperCase(), headers, bytes, sent ?? null]);
}

/** One view per `ArrayBuffer`, so every build passes the same bytes object for it. */
const views = new WeakMap<ArrayBuffer, Uint8Array>();

function bytesOf(src: Uint8Array | ArrayBuffer): Uint8Array {
  if (src instanceof Uint8Array) return src;
  let view = views.get(src);
  if (!view) {
    view = new Uint8Array(src);
    views.set(src, view);
  }
  return view;
}
