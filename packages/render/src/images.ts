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

/** Where images may come from: the remote and local policies of the render. */
export interface ImagePolicies {
  remote: RemoteImageOptions | undefined;
  local: LocalImageOptions | undefined;
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
    case 'deferred': {
      let value: ImageSrcValue;
      try {
        value = await (typeof image.src === 'function' ? image.src() : image.src);
      } catch (e) {
        throw new RenderError(
          'TEMPLATE_ERROR',
          `An <Image src> function or promise failed: ${(e as Error)?.message ?? String(e)}`,
          { cause: e },
        );
      }
      return resolveValue(value, registered, policies);
    }
  }
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
  if (typeof value === 'string') {
    const known = Object.hasOwn(registered, value) ? registered[value] : undefined;
    if (known) return known;
    if (value.startsWith('data:')) return dataUrlBytes(value) ?? fail('a data: URL that cannot be read');
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

/** The bytes of a `data:` URL, base64 or percent-encoded; undefined when it is malformed. */
function dataUrlBytes(url: string): Uint8Array | undefined {
  const comma = url.indexOf(',');
  if (comma < 0) return undefined;
  const body = url.slice(comma + 1);
  try {
    return /;base64$/i.test(url.slice(0, comma))
      ? Buffer.from(body, 'base64')
      : Buffer.from(decodeURIComponent(body), 'utf8');
  } catch {
    return undefined;
  }
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
