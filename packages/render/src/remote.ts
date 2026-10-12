import type { ImageRequest } from '@inkoxide/react';

import { DEFAULT_IMAGE_TIMEOUT_MS, DEFAULT_MAX_IMAGE_BYTES, envList, timerMs } from './policy.ts';
import type { RemoteImageOptions } from './types.ts';
import { RenderError } from './types.ts';

/** Where a refused URL's message says the allowlist can also be set. */
export const REMOTE_IMAGES_VARIABLE = 'the INKOXIDE_REMOTE_IMAGES variable';

/** The remote-image policy from the options, else from `INKOXIDE_REMOTE_IMAGES`. */
export function remotePolicy(opts: RemoteImageOptions | undefined): RemoteImageOptions | undefined {
  if (opts) return opts;
  const allow = envList('INKOXIDE_REMOTE_IMAGES', ',');
  return allow ? { allow } : undefined;
}

/**
 * Fetches one image from a URL or a request (already checked against the allowlist), within the
 * policy's timeout and size limit. Redirects are refused: their target could be outside the
 * allowlist.
 */
export function fetchImage(src: string | ImageRequest, opts: RemoteImageOptions): Promise<Uint8Array> {
  const url = typeof src === 'string' ? src : src.uri;
  const init: RequestInit = { redirect: 'manual' };
  if (typeof src !== 'string') {
    if (src.method !== undefined) init.method = src.method;
    if (src.headers !== undefined) init.headers = src.headers;
    if (src.body !== undefined) init.body = src.body as BodyInit;
  }
  const limits = {
    timeoutMs: opts.timeoutMs ?? DEFAULT_IMAGE_TIMEOUT_MS,
    maxBytes: opts.maxBytes ?? DEFAULT_MAX_IMAGE_BYTES,
  };
  return fetchBytes(url, init, limits, (why, cause) => {
    throw new RenderError('IMAGE_FETCH_FAILED', `Cannot fetch the image ${url}: ${why}.`, { cause });
  });
}

/** The statuses that redirect; another 3xx (a 304) is an HTTP error like a 4xx. */
const REDIRECTS = new Set([301, 302, 303, 307, 308]);

/** What one download may take: the whole download's time (`Infinity` for none), and its size. */
export interface FetchLimits {
  timeoutMs: number;
  maxBytes: number;
}

/**
 * The body of `url`, within `limits`. Every failure, before or during the body, throws what
 * `fail` makes of the reason (`HTTP 404`, `timed out after 10000 ms`). With `redirect: 'manual'`
 * in `init`, a redirect is a failure.
 */
export async function fetchBytes(
  url: string | URL,
  init: RequestInit,
  limits: FetchLimits,
  fail: (why: string, cause?: unknown) => never,
): Promise<Uint8Array> {
  const { timeoutMs, maxBytes } = limits;
  const ms = timerMs(timeoutMs);
  const reason = (e: unknown): string =>
    e instanceof Error && e.name === 'TimeoutError'
      ? `timed out after ${timeoutMs} ms`
      : e instanceof Error
        ? e.message
        : String(e);
  let res: Response;
  try {
    res = await fetch(url, ms === undefined ? init : { ...init, signal: AbortSignal.timeout(ms) });
  } catch (e) {
    return fail(reason(e), e);
  }
  if (init.redirect === 'manual' && REDIRECTS.has(res.status)) {
    return fail(`HTTP ${res.status}, and redirects are not followed`);
  }
  if (!res.ok) return fail(`HTTP ${res.status}`);
  const length = Number(res.headers.get('content-length') ?? 0);
  if (length > maxBytes) return fail(`${length} bytes, over the ${maxBytes}-byte limit`);
  const reader = res.body?.getReader();
  if (!reader) return fail('empty response');
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxBytes) break;
      chunks.push(value);
    }
  } catch (e) {
    return fail(reason(e), e);
  }
  if (size > maxBytes) {
    await reader.cancel();
    return fail(`over the ${maxBytes}-byte limit`);
  }
  const bytes = new Uint8Array(size);
  let at = 0;
  for (const c of chunks) {
    bytes.set(c, at);
    at += c.byteLength;
  }
  return bytes;
}
