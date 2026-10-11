import type { RemoteImageOptions } from './types.ts';
import { RenderError } from './types.ts';

/** The remote-image policy from the options, else from `INKOXIDE_REMOTE_IMAGES`. */
export function remotePolicy(opts: RemoteImageOptions | undefined): RemoteImageOptions | undefined {
  if (opts) return opts;
  const env = process.env.INKOXIDE_REMOTE_IMAGES;
  if (!env) return undefined;
  const allow = env
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  return allow.length > 0 ? { allow } : undefined;
}

/**
 * Fetches the remote images a tree lists (already checked against the allowlist), in parallel.
 * Redirects are refused, so an allowed URL cannot lead to a host that is not.
 */
export async function fetchRemote(
  remote: Record<string, string>,
  opts: RemoteImageOptions,
): Promise<Record<string, Uint8Array>> {
  const timeoutMs = opts.timeoutMs ?? 10_000;
  const maxBytes = opts.maxBytes ?? 20 * 1024 * 1024;
  const entries = await Promise.all(
    Object.entries(remote).map(async ([id, url]): Promise<[string, Uint8Array]> => {
      const fail = (why: string, cause?: unknown): never => {
        throw new RenderError('IMAGE_FETCH_FAILED', `Cannot fetch the image ${url}: ${why}.`, { cause });
      };
      let res: Response;
      try {
        res = await fetch(url, { redirect: 'error', signal: AbortSignal.timeout(timeoutMs) });
      } catch (e) {
        return fail(
          (e as Error).name === 'TimeoutError' ? `no answer in ${timeoutMs} ms` : (e as Error).message,
          e,
        );
      }
      if (!res.ok) return fail(`HTTP ${res.status}`);
      const length = Number(res.headers.get('content-length') ?? 0);
      if (length > maxBytes) return fail(`${length} bytes, over the ${maxBytes}-byte limit`);
      const reader = res.body?.getReader();
      if (!reader) return fail('empty response');
      const chunks: Uint8Array[] = [];
      let size = 0;
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        if (size > maxBytes) {
          await reader.cancel();
          return fail(`over the ${maxBytes}-byte limit`);
        }
        chunks.push(value);
      }
      const bytes = new Uint8Array(size);
      let at = 0;
      for (const c of chunks) {
        bytes.set(c, at);
        at += c.byteLength;
      }
      return [id, bytes];
    }),
  );
  return Object.fromEntries(entries);
}
