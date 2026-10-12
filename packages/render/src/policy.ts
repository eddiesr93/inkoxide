/** The limits of loading images and fonts, and the reading of the image environment variables. */

import { RenderError, type RenderOpts } from './types.ts';

/** Largest image file or download in bytes, unless the policy says otherwise: 20 MiB. */
export const DEFAULT_MAX_IMAGE_BYTES = 20 * 1024 * 1024;

/** Time for one download, and for a function or promise `src`, unless the options say otherwise. */
export const DEFAULT_IMAGE_TIMEOUT_MS = 10_000;

/** Time for one font download (`Font.register` with a URL). */
export const FONT_TIMEOUT_MS = 30_000;

/** Largest font download in bytes: 50 MiB. */
export const MAX_FONT_BYTES = 50 * 1024 * 1024;

/**
 * A timeout as a timer takes it: undefined for `Infinity` (no limit), otherwise a whole number
 * of ms (a fraction rounded up, as `AbortSignal.timeout` takes integers only) from 0 to
 * 2^31 - 1, since Node's timers fire at once past that.
 */
export function timerMs(ms: number): number | undefined {
  if (ms === Infinity) return undefined;
  return ms > 0 ? Math.min(Math.ceil(ms), 2 ** 31 - 1) : 0;
}

/**
 * Checks the image limits of the render options before anything loads: each timeout and size
 * must be a number from 0, `Infinity` included. NaN would fail every image at once, and a NaN
 * size would lift the limit. A bad one is an `ENGINE_REJECTED` RenderError (400) at its path,
 * as a render option the engine refuses is.
 */
export function checkImageLimits(opts: RenderOpts): void {
  check('imageTimeoutMs', opts.imageTimeoutMs, 'milliseconds from 0 (Infinity for no limit)');
  check(
    'remoteImages.timeoutMs',
    opts.remoteImages?.timeoutMs,
    'milliseconds from 0 (Infinity for no limit)',
  );
  check('remoteImages.maxBytes', opts.remoteImages?.maxBytes, 'bytes from 0');
  check('localImages.maxBytes', opts.localImages?.maxBytes, 'bytes from 0');
}

function check(name: string, value: unknown, unit: string): void {
  if (value === undefined || (typeof value === 'number' && value >= 0)) return;
  const found = typeof value === 'string' ? JSON.stringify(value) : String(value);
  throw new RenderError(
    'ENGINE_REJECTED',
    `The render option ${name} must be a number of ${unit}, found ${found}.`,
    { path: `options.${name}` },
  );
}

/**
 * The entries of the environment variable `name`, split at `separator` and trimmed; undefined
 * when it is unset or lists nothing.
 */
export function envList(name: string, separator: string): string[] | undefined {
  const env = process.env[name];
  if (!env) return undefined;
  const list = env
    .split(separator)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  return list.length > 0 ? list : undefined;
}
