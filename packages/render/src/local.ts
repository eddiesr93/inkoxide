/**
 * Image files, read only from the folders the caller allows (`localImages`). A path comes from the
 * template and may be untrusted data: it must lead, symbolic links followed, to a regular file
 * inside one of the folders. Errors name the path as the template wrote it, and say the same
 * whether a file is missing or outside the folders.
 */
import { constants } from 'node:fs';
import { open, realpath } from 'node:fs/promises';
import { delimiter, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { DEFAULT_MAX_IMAGE_BYTES, envList } from './policy.ts';
import type { LocalImageOptions } from './types.ts';
import { RenderError } from './types.ts';

/** The local-image policy from the options, else from `INKOXIDE_LOCAL_IMAGES`. */
export function localPolicy(opts: LocalImageOptions | undefined): LocalImageOptions | undefined {
  if (opts) return opts;
  const root = envList('INKOXIDE_LOCAL_IMAGES', delimiter);
  return root ? { root } : undefined;
}

// Opening a FIFO without O_NONBLOCK waits for a writer; O_NOFOLLOW refuses a file swapped for a
// symbolic link after the check. Neither exists on Windows.
const OPEN_FLAGS = constants.O_RDONLY | (constants.O_NONBLOCK ?? 0) | (constants.O_NOFOLLOW ?? 0);

/** True when `path` is strictly inside the folder `root` (both absolute). */
function inside(path: string, root: string): boolean {
  const rel = relative(root, path);
  return rel !== '' && rel !== '..' && !rel.startsWith(`..${sep}`) && !isAbsolute(rel);
}

/**
 * Reads the image file `src` names: a path relative to the first root, an absolute path or a
 * `file:` URL, inside one of the roots. Network paths (`\\host\share`, `//host/share`,
 * `file://host/…`) are refused, so a path never makes a network request.
 */
export async function readLocalImage(src: string, opts: LocalImageOptions): Promise<Uint8Array> {
  const maxBytes = opts.maxBytes ?? DEFAULT_MAX_IMAGE_BYTES;
  const fail = (why: string, cause?: unknown): never => {
    throw new RenderError('IMAGE_READ_FAILED', `Cannot read the image ${src}: ${why}.`, { cause });
  };
  const missing = () => fail('no such file in the localImages folders');
  const given = typeof opts.root === 'string' ? [opts.root] : opts.root;
  // An empty root would quietly mean the working directory.
  if (given.length === 0 || given.some((r) => r === '')) fail('a localImages folder cannot be opened');
  const roots = given.map((r) => resolve(r));

  let path = src;
  if (/^file:/i.test(src)) {
    let url: URL;
    try {
      url = new URL(src);
    } catch {
      return fail('not a valid file: URL');
    }
    if (url.hostname !== '' && url.hostname !== 'localhost') return fail('network paths are not allowed');
    try {
      path = fileURLToPath(url);
    } catch {
      return fail('not a valid file: URL');
    }
  }
  if (/^[\\/]{2}/.test(path)) return fail('network paths are not allowed');
  const first = roots[0];
  if (first === undefined) return missing();
  path = isAbsolute(path) ? resolve(path) : resolve(first, path);

  let realRoots: string[];
  try {
    realRoots = await Promise.all(roots.map((r) => realpath(r)));
  } catch (e) {
    // The caller's configuration, not the template's path: the cause may name the folder.
    return fail('a localImages folder cannot be opened', e);
  }
  // Paths outside every folder are refused before the file system sees them.
  if (!roots.some((r) => inside(path, r)) && !realRoots.some((r) => inside(path, r))) return missing();
  let real: string;
  try {
    real = await realpath(path);
  } catch {
    return missing();
  }
  if (!realRoots.some((r) => inside(real, r))) return missing();

  let handle: Awaited<ReturnType<typeof open>>;
  try {
    handle = await open(real, OPEN_FLAGS);
  } catch (e) {
    const code = (e as NodeJS.ErrnoException).code;
    if (code === 'EACCES' || code === 'EPERM') return fail('permission denied');
    return missing();
  }
  try {
    const stat = await handle.stat();
    if (!stat.isFile()) return fail('not a file');
    if (stat.size > maxBytes) return fail(`${stat.size} bytes, over the ${maxBytes}-byte limit`);
    const bytes = Buffer.allocUnsafe(stat.size);
    let at = 0;
    while (at < bytes.byteLength) {
      const { bytesRead } = await handle.read(bytes, at, bytes.byteLength - at, at);
      if (bytesRead === 0) break;
      at += bytesRead;
    }
    return bytes.subarray(0, at);
  } catch (e) {
    if (e instanceof RenderError) throw e;
    return fail('the file cannot be read');
  } finally {
    await handle.close();
  }
}
