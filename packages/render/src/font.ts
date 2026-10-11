/**
 * `Font.register`: adds a font family next to the bundled Inter, Noto Sans and
 * Roboto Mono. Registration only records the source; the file is read (or fetched) and handed
 * to the engine before the next render, so `register` stays synchronous.
 */
import { readFile } from 'node:fs/promises';

import { type FontWeightName, fonts, fontWeightValue } from '@inkoxide/react';

import { registerEngineFont } from './engine.ts';
import { RenderError } from './types.ts';

/** A TrueType or CFF OpenType font: a file path, a `file:` or `https://` URL, or its bytes. */
export type FontSrc = string | URL | Uint8Array | ArrayBuffer;

export type FontWeight = number | FontWeightName;

export interface FontSource {
  src: FontSrc;
  /** 100–900 or a CSS name (`'bold'`); default 400. */
  fontWeight?: FontWeight;
  fontStyle?: 'normal' | 'italic';
}

/** One face (`src`) or several (`fonts`) of a family. */
export type FontRegistration =
  | ({ family: string } & FontSource)
  | { family: string; fonts: readonly FontSource[] };

interface Pending {
  id: string;
  family: string;
  weight: number;
  style: 'normal' | 'italic';
  src: FontSrc;
}

const pending: Pending[] = [];
/** Faces registered through `Font.register`, by id, with the source they came from. */
const known = new Map<string, FontSrc>();
let loading: Promise<void> = Promise.resolve();

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

async function bytesOf(src: FontSrc): Promise<Uint8Array> {
  if (src instanceof Uint8Array) return src;
  if (src instanceof ArrayBuffer) return new Uint8Array(src);
  const url = src instanceof URL ? src : /^https?:\/\//i.test(src) ? new URL(src) : undefined;
  if (url && (url.protocol === 'http:' || url.protocol === 'https:')) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }
  return readFile(url ?? src);
}

export const Font = {
  /**
   * Adds a family: `Font.register({ family: 'Roboto', src: './Roboto-Regular.ttf' })`, or several
   * faces at once with `fonts: [{ src, fontWeight: 'bold' }, { src, fontStyle: 'italic' }]`.
   * Use it with `fontFamily: 'Roboto'` in a style or `font-[Roboto]` in a class. Registering the
   * same face again with the same source does nothing; with another source it throws.
   */
  register(registration: FontRegistration): void {
    const { family } = registration;
    if (typeof family !== 'string' || family.trim() === '')
      throw new Error('Font.register needs a family name.');
    const faces = 'fonts' in registration ? registration.fonts : [registration];
    if (faces.length === 0) throw new Error(`Font "${family}": fonts is empty.`);
    for (const face of faces) {
      if (face.src === undefined) throw new Error(`Font "${family}": every face needs a src.`);
      const weight = weightOf(face.fontWeight, family);
      const style = face.fontStyle ?? 'normal';
      const id = `${family.trim().replace(/\s+/g, '-')}-${weight}${style === 'italic' ? '-italic' : ''}`;
      const before = known.get(id);
      if (before !== undefined) {
        if (before === face.src || describe(before) === describe(face.src)) continue;
        throw new Error(
          `Font "${family}" ${weight} ${style} is already registered from ${describe(before)}.`,
        );
      }
      known.set(id, face.src);
      pending.push({ id, family, weight, style, src: face.src });
    }
  },
};

/**
 * Reads the fonts registered since the last render and hands them to the engine. Runs after
 * `initEngine` (which registers the bundled fonts first), one load at a time.
 */
export function loadRegisteredFonts(): Promise<void> {
  if (pending.length === 0) return loading;
  const batch = pending.splice(0);
  loading = loading.then(async () => {
    for (const f of batch) {
      let bytes: Uint8Array;
      try {
        bytes = await bytesOf(f.src);
        registerEngineFont(f.id, bytes);
      } catch (e) {
        known.delete(f.id);
        throw new RenderError(
          'INVALID_FONT',
          `Font "${f.family}" ${f.weight}${f.style === 'italic' ? ' italic' : ''} (${describe(f.src)}) could not be loaded: ${(e as Error).message}`,
          { cause: e },
        );
      }
      fonts.register({ id: f.id, family: f.family, weight: f.weight, style: f.style }, bytes);
    }
  });
  // A failed batch does not block later ones; the caller of this load sees the error.
  const result = loading;
  loading = loading.catch(() => {});
  return result;
}
