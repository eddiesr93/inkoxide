/** The defaults of the image policies, and their environment variables. */

/** Largest image file or download in bytes, unless the policy says otherwise: 20 MiB. */
export const DEFAULT_MAX_IMAGE_BYTES = 20 * 1024 * 1024;

/** Time for one download, and for a function or promise `src`, unless the options say otherwise. */
export const DEFAULT_IMAGE_TIMEOUT_MS = 10_000;

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
