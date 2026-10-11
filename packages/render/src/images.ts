/**
 * Loads the images a tree cannot carry itself: allowed remote URLs and requests, files, and
 * function or promise `src`s. Tree building stays synchronous; this runs before the engine.
 */
import {
  type ImageRequest,
  type ImageSrcValue,
  isImagePath,
  type PendingImage,
  remoteAllowed,
  type TreeResult,
} from '@inkoxide/react';

import { readLocalImage } from './local.ts';
import { fetchImage } from './remote.ts';
import { type LocalImageOptions, RenderError, type RemoteImageOptions } from './types.ts';

/** Where images may come from, and how long a function or promise `src` may take. */
export interface ImagePolicies {
  remote: RemoteImageOptions | undefined;
  local: LocalImageOptions | undefined;
  timeoutMs: number;
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
  const remote = Object.entries(tree.remote).map(async ([id, url]): Promise<[string, Uint8Array]> => [
    id,
    await fetchAllowed(url, policies.remote),
  ]);
  const pending = Object.entries(tree.pending).map(async ([id, image]): Promise<[string, Uint8Array]> => [
    id,
    await loadPending(image, registered, policies),
  ]);
  return Object.fromEntries(await Promise.all([...remote, ...pending]));
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
  let timer: ReturnType<typeof setTimeout> | undefined;
  let value: ImageSrcValue | typeof timedOut;
  try {
    value = await Promise.race([
      (async () => (typeof src === 'function' ? src() : src))(),
      new Promise<typeof timedOut>((ok) => {
        timer = setTimeout(() => ok(timedOut), timeoutMs);
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
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (typeof value === 'string') {
    const known = Object.hasOwn(registered, value) ? registered[value] : undefined;
    if (known) return known;
    if (value.startsWith('data:')) {
      return dataUrlBytes(value) ?? fail('a data: URL that is not valid base64 (`data:<type>;base64,…`)');
    }
    if (isImagePath(value)) return readFile(value, policies.local);
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return fetchAllowed(value, policies.remote);
    return fail(`"${value}", which is not an id in the images option, a path or a URL`);
  }
  if (typeof value === 'object' && value !== null && typeof (value as ImageRequest).uri === 'string') {
    return fetchAllowed(value as ImageRequest, policies.remote);
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
    throw new RenderError(
      'TEMPLATE_ERROR',
      `<Image src="${url}">: this URL is not in the remoteImages allowlist. Add its origin to the render option remoteImages.allow (or the INKOXIDE_REMOTE_IMAGES variable), or pass the bytes.`,
    );
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
  return Buffer.from(body, 'base64');
}

/** Reads an image file, when the render allows local images. */
async function readFile(path: string, policy: LocalImageOptions | undefined): Promise<Uint8Array> {
  if (!policy) {
    throw new RenderError(
      'TEMPLATE_ERROR',
      `<Image src="${path}">: image files are off. Allow their folder with the render option localImages.root (or the INKOXIDE_LOCAL_IMAGES variable), or pass the bytes.`,
    );
  }
  return readLocalImage(path, policy);
}
