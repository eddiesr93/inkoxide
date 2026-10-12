/** The limits of loading images and fonts, and the reading of the image environment variables. */

/** Largest image file or download in bytes, unless the policy says otherwise: 20 MiB. */
export const DEFAULT_MAX_IMAGE_BYTES = 20 * 1024 * 1024;

/** Time for one download, and for a function or promise `src`, unless the options say otherwise. */
export const DEFAULT_IMAGE_TIMEOUT_MS = 10_000;

/** Time for one font download (`Font.register` with a URL). */
export const FONT_TIMEOUT_MS = 30_000;

/** Largest font download in bytes: 50 MiB. */
export const MAX_FONT_BYTES = 50 * 1024 * 1024;

/**
 * A timeout as a timer takes it: undefined for `Infinity` (no limit), otherwise from 0 to
 * 2^31 - 1 ms, since Node's timers fire at once past that.
 */
export function timerMs(ms: number): number | undefined {
  if (ms === Infinity) return undefined;
  return ms > 0 ? Math.min(ms, 2 ** 31 - 1) : 0;
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
