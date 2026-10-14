/**
 * `Font.register`: adds a font family next to the bundled Inter, Inter Display, Noto Sans and
 * Roboto Mono. Registration only records the source; the file is read (or fetched) and handed
 * to the engine before the next render, so `register` stays synchronous.
 * `Font.registerHyphenationCallback`: the word breaking used by every render.
 */
import { readFile } from 'node:fs/promises';

import {
  type EmojiSource,
  type FontWeightName,
  fonts,
  fontWeightValue,
  type HyphenationCallback,
} from '@inkoxide/react';

import { registerEngineFont } from './engine.ts';
import { DEFAULT_FACES } from './fonts.ts';
import { FONT_TIMEOUT_MS, MAX_FONT_BYTES } from './policy.ts';
import { fetchBytes } from './remote.ts';
import { RenderError } from './types.ts';

/**
 * A TrueType, CFF OpenType, WOFF or WOFF2 font: a file path, a `file:`, `https:` or `http:` URL,
 * or its bytes.
 */
export type FontSrc = string | URL | Uint8Array | ArrayBuffer;

export type FontWeight = number | FontWeightName;

export interface FontSource {
  src: FontSrc;
  /** 1–1000 or a CSS name (`'bold'`); default 400. */
  fontWeight?: FontWeight;
  fontStyle?: 'normal' | 'italic';
}

/** One face (`src`) or several (`fonts`) of a family. */
export type FontRegistration =
  | ({ family: string } & FontSource)
  | { family: string; fonts: readonly FontSource[] };

/** A face of a family, as `Font.register` records it. */
interface Face {
  family: string;
  weight: number;
  style: 'normal' | 'italic';
  /** The engine's font id, made from the family, weight and style. */
  id: string;
  src: FontSrc;
}

type FaceName = Pick<Face, 'family' | 'weight' | 'style'>;

/** Family names match ignoring ASCII case, as in CSS and the font registry. */
const asciiLower = (s: string): string => s.replace(/[A-Z]/g, (c) => c.toLowerCase());

/** What tells faces apart: family (ignoring ASCII case), weight and style. */
const faceKey = (f: FaceName): string => JSON.stringify([asciiLower(f.family), f.weight, f.style]);

/** `"Brand" 700 italic`, as messages name a face. */
const faceLabel = (f: FaceName): string =>
  `"${f.family}" ${f.weight}${f.style === 'italic' ? ' italic' : ''}`;

const pending: Face[] = [];
/** Faces registered through `Font.register`, by `faceKey`. */
const known = new Map<string, Face>();
let loading: Promise<void> = Promise.resolve();
/** Batches handed to `loading` that have not finished. */
let batches = 0;

function weightOf(w: FontWeight | undefined, family: string): number {
  if (w === undefined) return 400;
  let n: number;
  try {
    n = fontWeightValue(w);
  } catch {
    n = Number.NaN;
  }
  if (!Number.isInteger(n) || n < 1 || n > 1000) {
    throw new Error(`Font "${family}": fontWeight ${JSON.stringify(w)} is not 1–1000 or a CSS weight name.`);
  }
  return n;
}

const describe = (src: FontSrc): string =>
  typeof src === 'string' ? src : src instanceof URL ? src.href : `${src.byteLength} bytes`;

const isFontSrc = (src: unknown): src is FontSrc =>
  typeof src === 'string' || src instanceof URL || src instanceof Uint8Array || src instanceof ArrayBuffer;

const bytesView = (src: FontSrc): Buffer | undefined =>
  src instanceof Uint8Array
    ? Buffer.from(src.buffer, src.byteOffset, src.byteLength)
    : src instanceof ArrayBuffer
      ? Buffer.from(src)
      : undefined;

/** Whether two sources are the same font: the same bytes, or the same path or URL. */
function sameSource(a: FontSrc, b: FontSrc): boolean {
  if (a === b) return true;
  const x = bytesView(a);
  const y = bytesView(b);
  if (x || y) return x !== undefined && y !== undefined && x.equals(y);
  return describe(a) === describe(b);
}

async function bytesOf(src: FontSrc): Promise<Uint8Array> {
  if (src instanceof Uint8Array) return src;
  if (src instanceof ArrayBuffer) return new Uint8Array(src);
  const url = src instanceof URL ? src : /^(?:https?|file):/i.test(src) ? new URL(src) : undefined;
  if (url && (url.protocol === 'http:' || url.protocol === 'https:')) {
    // The URL comes from the developer, not a template, so redirects are followed.
    const limits = { timeoutMs: FONT_TIMEOUT_MS, maxBytes: MAX_FONT_BYTES };
    return fetchBytes(url, {}, limits, (why, cause) => {
      throw new Error(why, { cause });
    });
  }
  return readFile(url ?? src);
}

export const Font = {
  /**
   * Adds a family: `Font.register({ family: 'Roboto', src: './Roboto-Regular.ttf' })`, or several
   * faces at once with `fonts: [{ src, fontWeight: 'bold' }, { src, fontStyle: 'italic' }]`.
   * Use it with `fontFamily: 'Roboto'` in a style or `font-[Roboto]` in a class. Registering the
   * same face (family, weight and style) again with the same source does nothing; with another
   * source it throws, and so does a face the bundled fonts have, or a family whose font id
   * another family has (`Brand Two` and `Brand-Two`). A registration that throws adds no face.
   */
  register(registration: FontRegistration): void {
    const { family } = registration;
    if (typeof family !== 'string' || family.trim() === '')
      throw new Error('Font.register needs a family name.');
    const faces = 'fonts' in registration ? registration.fonts : [registration];
    if (faces.length === 0) throw new Error(`Font "${family}": fonts is empty.`);
    const added: Face[] = [];
    for (const face of faces) {
      const { src } = face as { src: unknown };
      if (src === undefined) throw new Error(`Font "${family}": every face needs a src.`);
      if (!isFontSrc(src)) {
        const what = src === null ? 'null' : typeof src === 'object' ? 'an object' : `a ${typeof src}`;
        throw new Error(
          `Font "${family}": src must be a path, a URL or the bytes (Uint8Array or ArrayBuffer), not ${what}.`,
        );
      }
      const weight = weightOf(face.fontWeight, family);
      const style = face.fontStyle ?? 'normal';
      const id = `${family.trim().replace(/\s+/g, '-')}-${weight}${style === 'italic' ? '-italic' : ''}`;
      const f: Face = { family, weight, style, id, src };
      const bundled = DEFAULT_FACES.find(
        (d) =>
          asciiLower(d.family) === asciiLower(family) &&
          d.weight === weight &&
          (d.style ?? 'normal') === style,
      );
      if (bundled) {
        throw new Error(
          `Font ${faceLabel(f)} is bundled (${bundled.id}). Register yours under another family name.`,
        );
      }
      const before = known.get(faceKey(f)) ?? added.find((a) => faceKey(a) === faceKey(f));
      if (before) {
        if (sameSource(before.src, src)) continue;
        throw new Error(`Font ${faceLabel(f)} is already registered from ${describe(before.src)}.`);
      }
      const clash = [...known.values(), ...added].find((k) => k.id === id);
      if (clash) {
        throw new Error(
          `Font ${faceLabel(f)} would have the font id ${id}, which ${faceLabel(clash)} has. Use another family name.`,
        );
      }
      added.push(f);
    }
    for (const f of added) {
      known.set(faceKey(f), f);
      pending.push(f);
    }
  },

  /**
   * Breaks words where `callback` says, in every `Text` of later renders:
   * `Font.registerHyphenationCallback((word) => (word === 'Inkoxide' ? [word] : split(word)))`.
   * It returns the parts of a word in order; `[word]` keeps the word whole. It replaces the
   * built-in patterns (`hyphenate`); a `Text` can override it with its own
   * `hyphenationCallback`, and turns it off with `hyphenate={false}`. Registering again replaces
   * it; `null` removes it.
   */
  registerHyphenationCallback(callback: HyphenationCallback | null): void {
    if (callback !== null && typeof callback !== 'function') {
      throw new Error('Font.registerHyphenationCallback needs a function, or null to remove it.');
    }
    fonts.hyphenationCallback = callback ?? undefined;
  },

  /**
   * Draws emoji as images, in every `Text` of later renders: `{ url, format = 'png',
   * withVariationSelectors = false }` adds each emoji's code points (`1f469-200d-1f4bb`, without
   * `fe0f` unless asked) and `.format` to `url`; `{ builder }` turns those code points into any
   * image `src`. The images load like `<Image src>`: a URL must be allowed by `remoteImages`, and a
   * path needs `localImages`. An emoji whose image cannot be loaded is drawn by the fonts, with a
   * warning. Registering again replaces the source; `null` removes it.
   */
  registerEmojiSource(source: EmojiSource | null): void {
    const ok =
      source === null ||
      (typeof source === 'object' &&
        (('url' in source && typeof source.url === 'string') ||
          ('builder' in source && typeof source.builder === 'function')));
    if (!ok) {
      throw new Error(
        'Font.registerEmojiSource needs { url, format? } or { builder }, or null to remove it.',
      );
    }
    fonts.emojiSource = source ?? undefined;
  },
};

/**
 * Reads the fonts registered since the last render and hands them to the engine. Runs after
 * `initEngine` (which registers the bundled fonts first), one load at a time. A face that cannot
 * be loaded is dropped and fails this load, after the rest of the batch has loaded.
 */
export function loadRegisteredFonts(): Promise<void> {
  if (pending.length === 0) return loading;
  const batch = pending.splice(0);
  batches++;
  loading = loading.then(async () => {
    let failed: RenderError | undefined;
    try {
      for (const f of batch) {
        let bytes: Uint8Array;
        try {
          bytes = await bytesOf(f.src);
          registerEngineFont(f.id, bytes);
        } catch (e) {
          known.delete(faceKey(f));
          failed ??= new RenderError(
            'INVALID_FONT',
            `Font ${faceLabel(f)} (${describe(f.src)}) could not be loaded: ${(e as Error).message}`,
            { cause: e },
          );
          continue;
        }
        fonts.register({ id: f.id, family: f.family, weight: f.weight, style: f.style }, bytes);
      }
    } finally {
      batches--;
    }
    if (failed) throw failed;
  });
  // A failed batch does not block later ones; the caller of this load sees the error.
  const result = loading;
  loading = loading.catch(() => {});
  return result;
}

/** Whether faces registered with `Font.register` are not loaded yet (`init` loads them). */
export function fontsPending(): boolean {
  return pending.length > 0 || batches > 0;
}
