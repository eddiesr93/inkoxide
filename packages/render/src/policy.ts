/** The limits of loading images and fonts, and the reading of the image environment variables. */

import { nodeProcess } from './runtime.ts';
import { type Abortable, RenderError, type RenderOpts } from './types.ts';

/** Largest image file or download in bytes, unless the policy says otherwise: 20 MiB. */
export const DEFAULT_MAX_IMAGE_BYTES = 20 * 1024 * 1024;

/** The size limit of an image policy, remote or local: its `maxBytes`, else the default. */
export function maxImageBytes(policy: { maxBytes?: number }): number {
  return policy.maxBytes ?? DEFAULT_MAX_IMAGE_BYTES;
}

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

/**
 * Checks the types of the render options before anything renders, as `checkImageLimits` checks
 * the limits: the engine would refuse a value of another type without saying which option it is.
 * `null` is an option left out, except inside `encryption`, which goes to the engine as is. A bad one is an `ENGINE_REJECTED` RenderError (400) at its path;
 * the message names the type found, never the value, which may be a password.
 */
export function checkOptionTypes(opts: RenderOpts & Abortable): void {
  const o = opts as Record<string, unknown>;
  if (!isObject(o)) {
    throw new RenderError('ENGINE_REJECTED', `The render options must be an object, found ${typeName(o)}.`, {
      path: 'options',
    });
  }
  check('timeoutMs', opts.timeoutMs, 'milliseconds from 0 (Infinity for no limit)');
  checkTypes(o, ENGINE_OPTION_TYPES, '', true);
  if (!absent(o.pdfA) && typeof o.pdfA !== 'boolean' && typeof o.pdfA !== 'string') {
    rejectType('pdfA', 'a boolean or a string', o.pdfA);
  }
  for (const key of ['images', 'attachments'] as const) {
    const files = o[key];
    if (absent(files)) continue;
    if (!isObject(files)) rejectType(key, 'an object', files);
    for (const [id, bytes] of Object.entries(files)) {
      if (!(bytes instanceof Uint8Array)) rejectType(`${key}.${id}`, 'a Uint8Array', bytes);
    }
  }
  checkEncryptionTypes(o.encryption);
  if (!absent(o.localImages)) {
    const local = o.localImages;
    if (!isObject(local)) rejectType('localImages', 'an object', local);
    const root = local.root;
    const roots = Array.isArray(root) ? (root as unknown[]) : [root];
    if (roots.some((r) => typeof r !== 'string')) {
      rejectType('localImages.root', 'a folder or an array of folders', root);
    }
  }
  if (!absent(o.remoteImages)) {
    const remote = o.remoteImages;
    if (!isObject(remote)) rejectType('remoteImages', 'an object', remote);
    if (!Array.isArray(remote.allow) || remote.allow.some((a: unknown) => typeof a !== 'string')) {
      rejectType('remoteImages.allow', 'an array of strings', remote.allow);
    }
  }
}

/**
 * The types of an `encryption` option, the render option's or the one `<Document>` props make:
 * the passwords strings, the permissions booleans (`printing` a string).
 */
export function checkEncryptionTypes(encryption: unknown): void {
  if (absent(encryption)) return;
  if (!isObject(encryption)) rejectType('encryption', 'an object', encryption);
  checkTypes(encryption, { userPassword: 'string', ownerPassword: 'string' }, 'encryption.', false);
  const permissions = encryption.permissions;
  if (permissions === undefined) return;
  if (!isObject(permissions)) rejectType('encryption.permissions', 'an object', permissions);
  checkTypes(permissions, PERMISSION_TYPES, 'encryption.permissions.', false);
}

const ENGINE_OPTION_TYPES = {
  creationDate: 'string',
  threads: 'number',
  compress: 'boolean',
  subsetFonts: 'boolean',
} as const;

const PERMISSION_TYPES = {
  printing: 'string',
  modifying: 'boolean',
  copying: 'boolean',
  annotating: 'boolean',
  fillingForms: 'boolean',
  contentAccessibility: 'boolean',
  documentAssembly: 'boolean',
} as const;

function checkTypes(
  object: Record<string, unknown>,
  types: Record<string, 'string' | 'number' | 'boolean'>,
  prefix: string,
  nullIsLeftOut: boolean,
): void {
  for (const [key, type] of Object.entries(types)) {
    const v = object[key];
    if (v === undefined || (v === null && nullIsLeftOut)) continue;
    if (typeof v !== type) rejectType(`${prefix}${key}`, `a ${type}`, v);
  }
}

function rejectType(name: string, expected: string, value: unknown): never {
  throw new RenderError(
    'ENGINE_REJECTED',
    `The render option ${name} must be ${expected}, found ${typeName(value)}.`,
    { path: `options.${name}` },
  );
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function absent(v: unknown): v is null | undefined {
  return v === undefined || v === null;
}

function typeName(v: unknown): string {
  if (v === null) return 'null';
  if (v === undefined) return 'nothing';
  if (Array.isArray(v)) return 'an array';
  return typeof v === 'object' ? 'an object' : `a ${typeof v}`;
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
 * The environment variable `name`, where there is an environment (Node); undefined in
 * a browser.
 */
export function env(name: string): string | undefined {
  return nodeProcess()?.env?.[name];
}

/** Whether `NODE_ENV` is `production`. */
export const isProduction = (): boolean => env('NODE_ENV') === 'production';

/**
 * The entries of the environment variable `name`, split at `separator` and trimmed; undefined
 * when it is unset or lists nothing.
 */
export function envList(name: string, separator: string): string[] | undefined {
  const value = env(name);
  if (!value) return undefined;
  const list = value
    .split(separator)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  return list.length > 0 ? list : undefined;
}
