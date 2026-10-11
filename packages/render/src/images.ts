/**
 * Loads the images a tree cannot carry itself: allowed remote URLs and requests, files, and
 * function or promise `src`s. Tree building stays synchronous; this runs before the engine.
 */
import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  type ImageRequest,
  type ImageSrcValue,
  isImagePath,
  type PendingImage,
  remoteAllowed,
  type TreeResult,
} from '@inkoxide/react';

import { fetchImage, fetchRemote } from './remote.ts';
import { RenderError, type RemoteImageOptions } from './types.ts';

/**
 * The bytes of every remote and pending image of a tree, by id, loaded in parallel. `registered`
 * are the images passed in by id: a function or promise resolving to one of their ids gets it.
 */
export async function loadImages(
  tree: Pick<TreeResult, 'remote' | 'pending'>,
  registered: Record<string, Uint8Array>,
  policy: RemoteImageOptions | undefined,
): Promise<Record<string, Uint8Array>> {
  const pending = Object.entries(tree.pending);
  const hasRemote = policy !== undefined && Object.keys(tree.remote).length > 0;
  if (pending.length === 0 && !hasRemote) return {};
  const [remote, loaded] = await Promise.all([
    hasRemote ? fetchRemote(tree.remote, policy) : {},
    Promise.all(
      pending.map(async ([id, image]): Promise<[string, Uint8Array]> => [
        id,
        await loadPending(image, registered, policy),
      ]),
    ),
  ]);
  return { ...remote, ...Object.fromEntries(loaded) };
}

async function loadPending(
  image: PendingImage,
  registered: Record<string, Uint8Array>,
  policy: RemoteImageOptions | undefined,
): Promise<Uint8Array> {
  switch (image.kind) {
    case 'file':
      return readImageFile(image.path);
    case 'request':
      return fetchAllowed(image.request, policy);
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
      return resolveValue(value, registered, policy);
    }
  }
}

/** What a function or promise resolved to, read like the same `src` written in the template. */
async function resolveValue(
  value: unknown,
  registered: Record<string, Uint8Array>,
  policy: RemoteImageOptions | undefined,
): Promise<Uint8Array> {
  const fail = (why: string): never => {
    throw new RenderError('TEMPLATE_ERROR', `An <Image src> function or promise resolved to ${why}.`);
  };
  if (value instanceof Uint8Array) return value;
  if (typeof value === 'string') {
    const known = Object.hasOwn(registered, value) ? registered[value] : undefined;
    if (known) return known;
    if (value.startsWith('data:')) return dataUrlBytes(value) ?? fail('a data: URL that cannot be read');
    if (isImagePath(value)) return readImageFile(value);
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return fetchAllowed(value, policy);
    return fail(`"${value}", which is not an id in the images option, a path or a URL`);
  }
  if (typeof value === 'object' && value !== null && typeof (value as ImageRequest).uri === 'string') {
    return fetchAllowed(value as ImageRequest, policy);
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

const FILE_ERRORS: Record<string, string> = {
  ENOENT: 'no such file',
  ENOTDIR: 'no such file',
  EACCES: 'permission denied',
  EPERM: 'permission denied',
};

/**
 * Reads an image file: a path relative to the working directory or absolute, or a `file:` URL.
 * Only regular files: a directory, device or pipe is refused before it is opened.
 */
async function readImageFile(path: string): Promise<Uint8Array> {
  let full = path;
  const fail = (why: string, cause?: unknown): never => {
    const where = full === path ? path : `${path} (${full})`;
    throw new RenderError('IMAGE_READ_FAILED', `Cannot read the image ${where}: ${why}.`, { cause });
  };
  try {
    full = /^file:/i.test(path) ? fileURLToPath(new URL(path)) : resolve(path);
  } catch (e) {
    return fail((e as Error).message, e);
  }
  try {
    if (!(await stat(full)).isFile()) return fail('not a file');
    return await readFile(full);
  } catch (e) {
    if (e instanceof RenderError) throw e;
    const code = (e as NodeJS.ErrnoException).code;
    return fail((code && FILE_ERRORS[code]) ?? (e as Error).message, e);
  }
}
