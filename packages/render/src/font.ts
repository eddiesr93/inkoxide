/**
 * `Font.register`: adds a font family next to the bundled Inter, Inter Display, Noto Sans and
 * Roboto Mono. Registration only records the source; the file is read (or fetched) and handed
 * to the engine before the next render, so `register` stays synchronous.
 * `Font.registerHyphenationCallback`: the word breaking used by every render.
 */
import {
  type EmojiSource,
  familyKey,
  fontVariationsValue,
  type FontWeightName,
  fonts,
  type FontStretch,
  fontStretchValue,
  fontWeightValue,
  type HyphenationCallback,
} from '@inkoxide/react';

import { sameBytes } from './bytes.ts';
import { registerEngineFont } from './engine.ts';
import { DEFAULT_FACES } from './fonts.ts';
import { engineHost, NO_FILES } from './host.ts';
import { FONT_TIMEOUT_MS, MAX_FONT_BYTES } from './policy.ts';
import { fetchBytes } from './remote.ts';
import { onPage, pageUrl } from './runtime.ts';
import { RenderError } from './types.ts';

/**
 * A TrueType, CFF OpenType, WOFF or WOFF2 font: a file path, a `file:`, `https:` or `http:` URL,
 * its bytes, or a `Blob` (a `File` the page's user picked, say). In a browser, text is a URL,
 * relative to the page.
 */
export type FontSrc = string | URL | Uint8Array | ArrayBuffer | Blob;

export type FontWeight = number | FontWeightName;

export interface FontSource {
  src: FontSrc;
  /**
   * 1–1000 or a CSS name (`'bold'`); default 400. A variable font takes a range,
   * `[100, 900]`: each weight in it draws an instance of the font at that `wght`.
   */
  fontWeight?: FontWeight | readonly [FontWeight, FontWeight];
  fontStyle?: 'normal' | 'italic';
  /**
   * The width, a percentage of the normal one (`75`, `'75%'`) or a CSS keyword (`'condensed'`);
   * default 100. A variable font takes a range, `[75, 100]`: each width in it draws an instance of
   * the font at that `wdth`.
   */
  fontStretch?: FontStretch | readonly [FontStretch, FontStretch];
  /**
   * A variable font's axis values for every use of this face, as CSS `@font-face` takes them:
   * `{ ital: 1 }` or `"'slnt' -10"`, so one file can serve as the italic too.
   */
  fontVariationSettings?: string | Readonly<Record<string, number>>;
}

/** One face (`src`) or several (`fonts`) of a family. */
export type FontRegistration =
  | ({ family: string } & FontSource)
  | { family: string; fonts: readonly FontSource[] };

/** A face of a family, as `Font.register` records it. */
interface Face {
  family: string;
  weight: number;
  /** A variable font's weights. */
  weightRange?: readonly [number, number];
  style: 'normal' | 'italic';
  /** Its width, a percentage. */
  stretch: number;
  /** A variable font's widths. */
  stretchRange?: readonly [number, number];
  variations?: Readonly<Record<string, number>>;
  /** The engine's font id, made from the family, weight and style. */
  id: string;
  src: FontSrc;
}

/** What names a face: its family, weight, width and style, and a variable face's ranges. */
type FaceName = Pick<Face, 'family' | 'weight' | 'style' | 'stretch'> & {
  weightRange?: readonly [number, number];
  stretchRange?: readonly [number, number];
};

/**
 * What tells faces apart, as CSS matches them: family (ignoring ASCII case), weight and width (a
 * variable face's ranges) and style.
 */
const faceKey = (f: FaceName): string =>
  JSON.stringify([familyKey(f.family), f.weightRange ?? f.weight, f.style, f.stretchRange ?? f.stretch]);

/** The width of a face, as its label and font id name it: `75%`, `75-100%`, or nothing at 100%. */
const widthLabel = (f: FaceName): string =>
  f.stretchRange ? `${f.stretchRange.join('-')}%` : f.stretch === 100 ? '' : `${f.stretch}%`;

/** `"Brand" 700 75% italic`, as messages name a face. */
const faceLabel = (f: FaceName): string => {
  const width = widthLabel(f);
  return `"${f.family}" ${f.weightRange ? f.weightRange.join('–') : f.weight}${width ? ` ${width}` : ''}${f.style === 'italic' ? ' italic' : ''}`;
};

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

function stretchOf(s: FontStretch | undefined, family: string): number {
  if (s === undefined) return 100;
  try {
    return fontStretchValue(s);
  } catch {
    throw new Error(
      `Font "${family}": fontStretch ${JSON.stringify(s)} is not a width of 1% to 1000% or a CSS keyword.`,
    );
  }
}

/** A variable font's widths, `[75, 100]`, in order. */
function widthsOf(range: readonly unknown[], family: string): readonly [number, number] {
  if (range.length !== 2) throw new Error(`Font "${family}": a fontStretch range is two widths, [75, 100].`);
  const [a, b] = range.map((s) => stretchOf(s as FontStretch, family)) as [number, number];
  if (a > b)
    throw new Error(`Font "${family}": the fontStretch range [${a}, ${b}] goes from the narrower width.`);
  return [a, b];
}

/** A variable font's weights, `[100, 900]`, in order. */
function rangeOf(range: readonly unknown[], family: string): readonly [number, number] {
  if (range.length !== 2) throw new Error(`Font "${family}": a fontWeight range is two weights, [100, 900].`);
  const [a, b] = range.map((w) => weightOf(w as FontWeight, family)) as [number, number];
  if (a > b)
    throw new Error(`Font "${family}": the fontWeight range [${a}, ${b}] goes from the lower weight.`);
  return [a, b];
}

/** Whether `src` is a `Blob` (a `File` too), where there are blobs. */
const isBlob = (src: unknown): src is Blob => typeof Blob === 'function' && src instanceof Blob;

const describe = (src: FontSrc): string =>
  typeof src === 'string'
    ? src
    : src instanceof URL
      ? src.href
      : isBlob(src)
        ? 'name' in src && typeof src.name === 'string' && src.name !== ''
          ? `the file ${src.name}`
          : `a Blob of ${src.size} bytes`
        : `${src.byteLength} bytes`;

const isFontSrc = (src: unknown): src is FontSrc =>
  typeof src === 'string' ||
  src instanceof URL ||
  src instanceof Uint8Array ||
  src instanceof ArrayBuffer ||
  isBlob(src);

const bytesView = (src: FontSrc): Uint8Array | undefined =>
  src instanceof Uint8Array ? src : src instanceof ArrayBuffer ? new Uint8Array(src) : undefined;

/** Whether two sources are the same font: the same bytes, or the same path or URL. */
function sameSource(a: FontSrc, b: FontSrc): boolean {
  if (a === b) return true;
  // Two blobs are read when the font loads: the same one only.
  if (isBlob(a) || isBlob(b)) return false;
  const x = bytesView(a);
  const y = bytesView(b);
  if (x || y) return x !== undefined && y !== undefined && sameBytes(x, y);
  return describe(a) === describe(b);
}

async function bytesOf(src: FontSrc): Promise<Uint8Array> {
  if (src instanceof Uint8Array) return src;
  if (src instanceof ArrayBuffer) return new Uint8Array(src);
  if (isBlob(src)) return new Uint8Array(await src.arrayBuffer());
  const files = engineHost().files;
  // Without files (in a browser), text is a URL, relative to the page; a file path is an error
  // (`file:`, a drive letter or a backslash).
  if (!files && typeof src === 'string' && src.includes('\\')) throw new Error(NO_FILES);
  const url =
    src instanceof URL
      ? src
      : /^(?:https?|file):/i.test(src)
        ? new URL(src)
        : !files && pageUrl() !== undefined
          ? onPage(src)
          : undefined;
  if (url && (url.protocol === 'http:' || url.protocol === 'https:')) {
    // The URL comes from the developer, not a template, so redirects are followed.
    const limits = { timeoutMs: FONT_TIMEOUT_MS, maxBytes: MAX_FONT_BYTES };
    return fetchBytes(url, {}, limits, (why, cause) => {
      throw new Error(why, { cause });
    });
  }
  if (!files) throw new Error(NO_FILES);
  return files.readFile(url ?? src);
}

export const Font = {
  /**
   * Adds a family: `Font.register({ family: 'Roboto', src: './Roboto-Regular.ttf' })`, or several
   * faces at once with `fonts: [{ src, fontWeight: 'bold' }, { src, fontStyle: 'italic' }]`.
   * Use it with `fontFamily: 'Roboto'` in a style or `font-[Roboto]` in a class. Registering the
   * same face (family, weight and style) again with the same source does nothing; with another
   * source it throws, and so does a face the bundled fonts have, or a face whose font id a face
   * of another family has (`Brand Two` and `Brand-Two`, same weight and style). A registration
   * that throws adds no face.
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
          `Font "${family}": src must be a path, a URL, the bytes (Uint8Array or ArrayBuffer) or a Blob, not ${what}.`,
        );
      }
      const fw = face.fontWeight;
      const weightRange = Array.isArray(fw) ? rangeOf(fw, family) : undefined;
      const weight = weightRange
        ? Math.min(Math.max(400, weightRange[0]), weightRange[1])
        : weightOf(fw as FontWeight | undefined, family);
      const style = face.fontStyle ?? 'normal';
      const fs = face.fontStretch;
      const stretchRange = Array.isArray(fs) ? widthsOf(fs, family) : undefined;
      const stretch = stretchRange
        ? Math.min(Math.max(100, stretchRange[0]), stretchRange[1])
        : stretchOf(fs as FontStretch | undefined, family);
      const weights = weightRange ? weightRange.join('-') : String(weight);
      const named: FaceName = { family, weight, style, stretch };
      if (stretchRange) named.stretchRange = stretchRange;
      const width = widthLabel(named);
      const id = `${family.trim().replace(/\s+/g, '-')}-${weights}${width ? `-${width}` : ''}${style === 'italic' ? '-italic' : ''}`;
      const f: Face = { family, weight, style, stretch, id, src };
      if (weightRange) f.weightRange = weightRange;
      if (stretchRange) f.stretchRange = stretchRange;
      if (face.fontVariationSettings !== undefined) {
        try {
          f.variations = fontVariationsValue(face.fontVariationSettings);
        } catch (e) {
          throw new Error(`Font ${faceLabel(f)}: ${(e as Error).message}.`);
        }
      }
      const bundled = DEFAULT_FACES.find(
        (d) =>
          familyKey(d.family) === familyKey(family) &&
          d.weight === weight &&
          (d.style ?? 'normal') === style &&
          stretch === 100,
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
 * be loaded is dropped and fails this load, after the rest of the batch has loaded; later loads
 * do not try it again, so later renders fall back from it, until it is registered again.
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
          await registerEngineFont(f.id, bytes);
        } catch (e) {
          known.delete(faceKey(f));
          failed ??= new RenderError(
            'INVALID_FONT',
            `Font ${faceLabel(f)} (${describe(f.src)}) could not be loaded: ${(e as Error).message}`,
            { cause: e },
          );
          continue;
        }
        fonts.register(
          {
            id: f.id,
            family: f.family,
            weight: f.weight,
            style: f.style,
            ...(f.stretch !== 100 ? { stretch: f.stretch } : {}),
            ...(f.weightRange ? { weightRange: f.weightRange } : {}),
            ...(f.stretchRange ? { stretchRange: f.stretchRange } : {}),
            ...(f.variations ? { variations: f.variations } : {}),
          },
          bytes,
        );
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
