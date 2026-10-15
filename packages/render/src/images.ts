/**
 * Loads the images a tree cannot carry itself: allowed remote URLs and requests, files, and
 * function or promise `src`s. Tree building stays synchronous; this runs before the engine.
 */
import {
  type ImageRequest,
  imageSource,
  type ImageSrcValue,
  type PendingImage,
  remoteAllowed,
  remoteRefused,
  type TreeResult,
} from '@inkoxide/react';

import { base64Bytes } from './bytes.ts';
import { engineHost, NO_FILES } from './host.ts';
import { DEFAULT_IMAGE_TIMEOUT_MS, DEFAULT_MAX_IMAGE_BYTES, timerMs } from './policy.ts';
import { fetchImage, REMOTE_IMAGES_VARIABLE } from './remote.ts';
import { type LocalImageOptions, RenderError, type RemoteImageOptions } from './types.ts';

/** Where images may come from, and how long a function or promise `src` may take. */
export interface ImagePolicies {
  remote: RemoteImageOptions | undefined;
  local: LocalImageOptions | undefined;
  timeoutMs: number;
}

/** The loads of the remote and pending images of a tree, by id; `fetchUrl` fetches a remote one. */
function loads(
  tree: Pick<TreeResult, 'remote' | 'pending'>,
  registered: Record<string, Uint8Array>,
  policies: ImagePolicies,
  fetchUrl: (url: string, policy: RemoteImageOptions | undefined) => Promise<Uint8Array>,
): Array<[string, Promise<Uint8Array>]> {
  const remote = Object.entries(tree.remote).map(([id, url]): [string, Promise<Uint8Array>] => [
    id,
    fetchUrl(url, policies.remote),
  ]);
  const pending = Object.entries(tree.pending).map(([id, image]): [string, Promise<Uint8Array>] => [
    id,
    loadPending(image, registered, policies),
  ]);
  return [...remote, ...pending];
}

/**
 * The bytes of every remote and pending image of a tree, by id, loaded in parallel. `registered`
 * are the images passed in by id: a function or promise resolving to one of their ids gets it.
 */
export async function loadImages(
  tree: Pick<TreeResult, 'remote' | 'pending'>,
  registered: Record<string, Uint8Array>,
  policies: ImagePolicies,
): Promise<Record<string, Uint8Array>> {
  const all = loads(tree, registered, policies, fetchAllowed);
  const bytes = await Promise.all(all.map(([, load]) => load));
  return Object.fromEntries(all.map(([id], i) => [id, bytes[i] as Uint8Array]));
}

/**
 * Bytes by key, least recently used first out once they take more than `budget` bytes in all.
 * An entry over a sixteenth of the budget is not kept.
 */
export class ByteCache {
  private readonly entries = new Map<string, Uint8Array>();
  private total = 0;

  constructor(readonly budget: number) {}

  /** The bytes kept. */
  get size(): number {
    return this.total;
  }

  get(key: string): Uint8Array | undefined {
    const hit = this.entries.get(key);
    if (hit) {
      // A Map iterates in insertion order: a hit moves to the end, the most recently used.
      this.entries.delete(key);
      this.entries.set(key, hit);
    }
    return hit;
  }

  set(key: string, bytes: Uint8Array): void {
    const old = this.entries.get(key);
    if (old) {
      this.entries.delete(key);
      this.total -= old.byteLength;
    }
    if (bytes.byteLength > this.budget / 16) return;
    this.entries.set(key, bytes);
    this.total += bytes.byteLength;
    for (const [k, v] of this.entries) {
      if (this.total <= this.budget) break;
      this.entries.delete(k);
      this.total -= v.byteLength;
    }
  }
}

/** Emoji images fetched from URLs, kept across renders: the same few hundred come back. */
const emojiCache = new ByteCache(16 << 20);
/** Emoji downloads under way, shared by the renders that want the same URL meanwhile. */
const emojiFetches = new Map<string, Promise<Uint8Array>>();

/**
 * An emoji image from an allowed URL: kept, or being fetched for another render, or fetched now.
 * Whichever, it is within this render's timeout and size limit: a render that joins another's
 * download stops waiting for it at its own timeout, and the download goes on for the others.
 */
async function fetchEmoji(url: string, policy: RemoteImageOptions | undefined): Promise<Uint8Array> {
  if (!policy || !remoteAllowed(url, policy)) return fetchAllowed(url, policy);
  let load = emojiFetches.get(url);
  const hit = load ? undefined : emojiCache.get(url);
  if (!load && !hit) {
    load = fetchImage(url, policy)
      .then((bytes) => {
        emojiCache.set(url, bytes);
        return bytes;
      })
      .finally(() => emojiFetches.delete(url));
    emojiFetches.set(url, load);
  }
  const bytes = hit ?? (await within(load as Promise<Uint8Array>, url, policy));
  const max = policy.maxBytes ?? DEFAULT_MAX_IMAGE_BYTES;
  if (bytes.byteLength > max) {
    throw new RenderError(
      'IMAGE_FETCH_FAILED',
      `Cannot fetch the image ${url}: ${bytes.byteLength} bytes, over the ${max}-byte limit.`,
    );
  }
  return bytes;
}

/** `load`, or a timeout `IMAGE_FETCH_FAILED` once the policy's timeout has passed. */
async function within(
  load: Promise<Uint8Array>,
  url: string,
  policy: RemoteImageOptions,
): Promise<Uint8Array> {
  const timeoutMs = policy.timeoutMs ?? DEFAULT_IMAGE_TIMEOUT_MS;
  const limit = timerMs(timeoutMs);
  if (limit === undefined) return load;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      load,
      new Promise<never>((_, fail) => {
        timer = setTimeout(
          () =>
            fail(
              new RenderError(
                'IMAGE_FETCH_FAILED',
                `Cannot fetch the image ${url}: timed out after ${timeoutMs} ms.`,
              ),
            ),
          limit,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * The emoji images of a tree, like `loadImages`, but an image that cannot be loaded is left out
 * with the reason, instead of failing the render: its emoji are then drawn by the fonts. Images
 * fetched from URLs are kept across renders.
 */
export async function loadEmojiImages(
  tree: Pick<TreeResult, 'remote' | 'pending'>,
  registered: Record<string, Uint8Array>,
  policies: ImagePolicies,
): Promise<{ loaded: Record<string, Uint8Array>; failed: Record<string, string> }> {
  const all = loads(tree, registered, policies, fetchEmoji);
  const loaded: Record<string, Uint8Array> = {};
  const failed: Record<string, string> = {};
  (await Promise.allSettled(all.map(([, load]) => load))).forEach((r, i) => {
    const id = all[i]?.[0] as string;
    if (r.status === 'fulfilled') loaded[id] = r.value;
    else failed[id] = (r.reason as Error)?.message ?? String(r.reason);
  });
  return { loaded, failed };
}

async function loadPending(
  image: PendingImage,
  registered: Record<string, Uint8Array>,
  policies: ImagePolicies,
): Promise<Uint8Array> {
  switch (image.kind) {
    case 'file':
      return readFile(image.path, policies.local);
    case 'request':
      return fetchAllowed(image.request, policies.remote);
    case 'deferred':
      return resolveValue(await settle(image.src, policies.timeoutMs), registered, policies);
  }
}

/** Calls a function `src` or awaits a promise, within `timeoutMs`. */
async function settle(
  src: Extract<PendingImage, { kind: 'deferred' }>['src'],
  timeoutMs: number,
): Promise<ImageSrcValue> {
  const timedOut = Symbol('timeout');
  const limit = timerMs(timeoutMs);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let value: ImageSrcValue | typeof timedOut;
  try {
    value = await Promise.race([
      (async () => (typeof src === 'function' ? src() : src))(),
      new Promise<typeof timedOut>((ok) => {
        if (limit !== undefined) timer = setTimeout(() => ok(timedOut), limit);
      }),
    ]);
  } catch (e) {
    throw new RenderError(
      'TEMPLATE_ERROR',
      `An <Image src> function or promise failed: ${(e as Error)?.message ?? String(e)}`,
      { cause: e },
    );
  } finally {
    clearTimeout(timer);
  }
  if (value === timedOut) {
    throw new RenderError(
      'TEMPLATE_ERROR',
      `An <Image src> function or promise did not settle in ${timeoutMs} ms. Raise the render option imageTimeoutMs if it needs longer.`,
    );
  }
  return value;
}

/** What a function or promise resolved to, read like the same `src` written in the template. */
async function resolveValue(
  value: unknown,
  registered: Record<string, Uint8Array>,
  policies: ImagePolicies,
): Promise<Uint8Array> {
  const fail = (why: string): never => {
    throw new RenderError('TEMPLATE_ERROR', `An <Image src> function or promise resolved to ${why}.`);
  };
  const src = imageSource(value, (id) => Object.hasOwn(registered, id) && Boolean(registered[id]));
  switch (src.kind) {
    case 'bytes':
      return src.bytes instanceof Uint8Array ? src.bytes : new Uint8Array(src.bytes);
    case 'registered':
      return registered[src.src] as Uint8Array;
    case 'data':
      return dataUrlBytes(src.src) ?? fail('a data: URL that is not valid base64 (`data:<type>;base64,…`)');
    case 'path':
      return readFile(src.src, policies.local);
    case 'url':
      return fetchAllowed(src.src, policies.remote);
    case 'id':
      return fail(`"${src.src}", which is not an id in the images option, a path or a URL`);
    case 'request':
      if (typeof src.request.uri === 'string') return fetchAllowed(src.request, policies.remote);
      break;
  }
  return fail(`${describe(value)}; expected bytes, a string or a request`);
}

const describe = (v: unknown): string =>
  v === null ? 'null' : typeof v === 'object' ? (v.constructor?.name ?? 'an object') : typeof v;

async function fetchAllowed(
  src: string | ImageRequest,
  policy: RemoteImageOptions | undefined,
): Promise<Uint8Array> {
  const url = typeof src === 'string' ? src : src.uri;
  if (!policy || !remoteAllowed(url, policy)) {
    throw new RenderError('TEMPLATE_ERROR', remoteRefused(`src="${url}"`, REMOTE_IMAGES_VARIABLE));
  }
  return fetchImage(src, policy);
}

/**
 * The bytes of a `data:` URL, decoded as the engine decodes one written in `src`: base64 only,
 * ASCII whitespace ignored, padding optional. Undefined for a URL the engine refuses.
 */
function dataUrlBytes(url: string): Uint8Array | undefined {
  const at = url.indexOf(';base64,');
  if (at < 0) return undefined;
  const body = url.slice(at + ';base64,'.length).replace(/[ \t\n\f\r]/g, '');
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(body) || body.replace(/=+$/, '').length % 4 === 1) return undefined;
  return base64Bytes(body);
}

/** Reads an image file, when the render allows local images. */
async function readFile(path: string, policy: LocalImageOptions | undefined): Promise<Uint8Array> {
  if (!policy) {
    throw new RenderError(
      'TEMPLATE_ERROR',
      `<Image src="${path}">: image files are off. Allow their folder with the render option localImages.root (or the INKOXIDE_LOCAL_IMAGES variable), or pass the bytes.`,
    );
  }
  const files = engineHost().files;
  if (!files) throw new RenderError('TEMPLATE_ERROR', `<Image src="${path}">: ${NO_FILES}`);
  return files.readLocalImage(path, policy);
}
