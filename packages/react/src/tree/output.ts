// What a `<Document>` sets besides the tree: its PDF/A level and its encryption.
import type { Permissions } from '../components/index.ts';

/** Throws the template error of a prop, with `message`. */
type Fail = (message: string) => never;

export interface OutputOptions {
  /** The PDF/A level of `<Document conformance>`; the render option `pdfA` wins over it. */
  pdfA?: PdfALevel;
  /** `<Document tagged>`: tagged PDF. */
  tagged?: boolean;
  /** `<Document pdfUA>`: PDF/UA-1. */
  pdfUA?: boolean;
  /** `<Document flattenAlpha>`: images with partial alpha drawn on white at PDF/A-1b. */
  flattenAlpha?: boolean;
  /**
   * The encryption of `<Document userPassword ownerPassword permissions entropy>`; the render option
   * `encryption` replaces it. JSON and `util.inspect` show it as `'<redacted>'`.
   */
  encryption?: EncryptionOptions;
}

/** The encryption render option (the `inkoxide` binding's `EncryptionOptions`). */
export interface EncryptionOptions {
  userPassword?: string;
  ownerPassword?: string;
  permissions?: Permissions;
  /** Random bytes mixed into the keys; `true`: 32 drawn for each render. */
  entropy?: boolean | Uint8Array;
}

const REDACTED = '<redacted>';

/** Output options whose JSON and `util.inspect` view hide the passwords. */
export function outputOptions(): OutputOptions {
  const redacted = function (this: OutputOptions): object {
    return this.encryption === undefined ? { ...this } : { ...this, encryption: REDACTED };
  };
  const out: OutputOptions = {};
  for (const key of ['toJSON', Symbol.for('nodejs.util.inspect.custom')]) {
    Object.defineProperty(out, key, { value: redacted, enumerable: false });
  }
  return out;
}

/** The PDF/A parts and conformance levels the engine writes, from the oldest part. */
const PDF_A_LEVELS = ['1b', '2b', '2a', '3b', '3a'] as const;

/** A PDF/A part and conformance level the engine writes. */
export type PdfALevel = (typeof PDF_A_LEVELS)[number];

/**
 * The encryption of `<Document userPassword ownerPassword permissions entropy>`, its types
 * checked; the engine checks the values. Messages never show a password.
 */
export function documentEncryption(p: Record<string, unknown>, fail: Fail): EncryptionOptions | undefined {
  const out: EncryptionOptions = {};
  for (const key of ['userPassword', 'ownerPassword'] as const) {
    const v = p[key];
    if (v === undefined) continue;
    if (typeof v !== 'string') fail(`<Document ${key}> must be a string.`);
    out[key] = v;
  }
  if (p.permissions !== undefined) {
    const perms = p.permissions;
    if (typeof perms !== 'object' || perms === null || Array.isArray(perms)) {
      fail('<Document permissions> must be an object.');
    }
    out.permissions = { ...(perms as Permissions) };
  }
  const entropy = p.entropy;
  if (entropy !== undefined) {
    if (typeof entropy !== 'boolean' && !(entropy instanceof Uint8Array)) {
      fail('<Document entropy> must be a boolean or a Uint8Array.');
    }
    if (entropy !== false) out.entropy = entropy instanceof Uint8Array ? entropy.slice() : true;
  }
  return Object.keys(out).length > 0 ? out : undefined;
}

/** The PDF/A level of `<Document conformance>`; a bare part (`'PDF/A-2'`) is level b. */
export function documentConformance(value: unknown, fail: Fail): PdfALevel {
  const m = typeof value === 'string' ? /^PDF\/A-(\d)([abu]?)$/.exec(value) : null;
  const level = PDF_A_LEVELS.find((l) => l === `${m?.[1]}${m?.[2] || 'b'}`);
  if (m && level) return level;
  const shown = typeof value === 'string' ? `"${value}"` : `{${String(value)}}`;
  if (m && PDF_A_LEVELS.some((l) => l === `${m[1]}b`)) {
    fail(
      `<Document conformance=${shown}>: the u levels and PDF/A-1a are not supported; use 'PDF/A-${m[1]}b'${m[1] === '1' ? '' : ` or 'PDF/A-${m[1]}a'`}.`,
    );
  }
  const levels = PDF_A_LEVELS.map((l) => `'PDF/A-${l}'`);
  const parts = PDF_A_LEVELS.filter((l) => l.endsWith('b')).map((l) => `'PDF/A-${l.slice(0, -1)}'`);
  fail(
    `<Document conformance=${shown}> is not a PDF/A level: use ${levels.slice(0, -1).join(', ')} or ${levels.at(-1)} (or ${parts.join(', ')}).`,
  );
}
