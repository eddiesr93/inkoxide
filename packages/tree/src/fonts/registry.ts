import type { FontDecl } from '@inkoxide/schema';

import type { HyphenationCallback, ImageSrcValue } from '../components/index.ts';

/**
 * Where emoji images come from (`Font.registerEmojiSource`): a base URL to which the code points
 * of each emoji (`1f469-200d-1f4bb`) and `.format` are added, or a function from those code
 * points to an image `src`.
 */
export type EmojiSource =
  | {
      url: string;
      /** The file extension. Default `png`. */
      format?: string;
      /** Keep U+FE0F (`fe0f`) in the code points. Default: left out, as most emoji sets name files. */
      withVariationSelectors?: boolean;
    }
  | { builder: (codePoints: string) => ImageSrcValue };

/** The weights a face draws, lowest and highest: one weight twice for a static face. */
type Weights = readonly [lo: number, hi: number];
/** The widths a face draws, as percentages, lowest and highest: one width twice for a static face. */
type Widths = readonly [lo: number, hi: number];

export interface FontFace {
  /** The engine font id, used in `fonts[].id` and `runs[].font` (`Inter-Bold`). */
  id: string;
  family: string;
  weight?: number;
  /**
   * A variable font's weights, `[100, 900]`: a weight in the range draws at that `wght`, and one
   * outside it at the nearer end. `weight` is then the weight of its default instance.
   */
  weightRange?: Weights;
  style?: 'normal' | 'italic';
  /** Its width, a percentage of the normal one (`75` is condensed). Default 100. */
  stretch?: number;
  /**
   * A variable font's widths, `[75, 100]`: a width in the range draws at that `wdth`, as
   * `weightRange` draws weights. `stretch` is then the width of its default instance.
   */
  stretchRange?: Widths;
  /** Axis values every use of the face draws with (`{ ital: 1 }`), as CSS `@font-face` takes them. */
  variations?: Readonly<Record<string, number>>;
  /**
   * File name, for `loadDirectory`.
   * @deprecated `loadDirectory` is deprecated: read the file and pass its bytes to `register`.
   */
  file?: string;
}

export interface ResolvedFont {
  /** The font id: a registered face's, or an instance of a variable face (`Inter-100-900@wght=650`). */
  id: string;
  /** False when the weight, width, style or family had to fall back. */
  exact: boolean;
  /** What was asked for, when it did not match exactly. */
  wanted?: string;
}

interface Face extends Required<Omit<FontFace, 'file' | 'weightRange' | 'stretchRange' | 'variations'>> {
  bytes: Uint8Array | undefined;
  /** The weights it draws: one, or a variable font's range. */
  weights: Weights;
  /** The widths it draws: one, or a variable font's range. */
  widths: Widths;
  variable: boolean;
  variations: Readonly<Record<string, number>>;
}

/** An instance of a variable face: the face, at these axis values. */
interface Instance {
  face: Face;
  /** The weight it declares, for SVG text to match. */
  weight: number;
  /** The width it declares, for SVG text to match. */
  stretch: number;
  variations: Readonly<Record<string, number>>;
}

/**
 * Most instances, and resolved fonts, kept: a document declares one per weight and variation it
 * uses (the engine takes 64), and a long-running server meets many more over time, so the
 * least recently used make room.
 */
const MAX_KEPT_FONTS = 4096;

/**
 * Maps family + width + style + weight to registered font ids, and holds the font bytes until the
 * engine registers them. Matching follows the CSS Fonts 4 algorithm: the first registered family
 * of the stack (ignoring ASCII case), then the nearest width, the style, the nearest weight. The engine's
 * `FontSet::select` matches the same way; `fixtures/fonts/matching.json` holds the cases both
 * are tested with.
 */
export class FontRegistry {
  private readonly faces: Face[] = [];
  private readonly byId = new Map<string, Face>();
  private readonly memo = new Map<string, ResolvedFont>();
  private readonly instances = new Map<string, Instance>();
  /** Splits words into syllables in every `Text` without its own `hyphenationCallback`. */
  hyphenationCallback: HyphenationCallback | undefined;
  /** Draws the emoji of every `Text` as images from here. */
  emojiSource: EmojiSource | undefined;

  register(face: FontFace, bytes?: Uint8Array): this {
    if (this.byId.has(face.id)) throw new Error(`Font id "${face.id}" is already registered`);
    if (face.id.includes('@')) throw new Error(`Font id "${face.id}" has an @, which instances use`);
    const range = face.weightRange;
    const weight = face.weight ?? (range ? clamp(400, range) : 400);
    const widths = face.stretchRange;
    const stretch = face.stretch ?? (widths ? clamp(100, widths) : 100);
    const f: Face = {
      id: face.id,
      family: face.family,
      weight,
      style: face.style ?? 'normal',
      stretch,
      bytes,
      weights: range ? [range[0], range[1]] : [weight, weight],
      widths: widths ? [widths[0], widths[1]] : [stretch, stretch],
      variable: range !== undefined || widths !== undefined || face.variations !== undefined,
      variations: face.variations ?? {},
    };
    this.faces.push(f);
    this.byId.set(f.id, f);
    this.memo.clear();
    return this;
  }

  /**
   * Registers every face whose `file` is in `dir`, skipping ids already registered, and returns
   * the ids it registered (`loaded`) and the ids whose file it could not read (`missing`). Node
   * only.
   * @deprecated Read the files and pass their bytes to `register`.
   */
  loadDirectory(dir: string, faces: readonly FontFace[]): { loaded: string[]; missing: string[] } {
    // Not imported, so that the package loads where there is no `node:fs`.
    const process = (globalThis as { process?: { getBuiltinModule?(id: string): unknown } }).process;
    const fs = process?.getBuiltinModule?.('node:fs') as typeof import('node:fs') | undefined;
    const path = process?.getBuiltinModule?.('node:path') as typeof import('node:path') | undefined;
    if (!fs || !path) throw new Error('fonts.loadDirectory reads files, which only Node can do here.');
    const loaded: string[] = [];
    const missing: string[] = [];
    for (const face of faces) {
      if (!face.file || this.byId.has(face.id)) continue;
      let bytes: Uint8Array;
      try {
        bytes = fs.readFileSync(path.join(dir, face.file));
      } catch {
        missing.push(face.id);
        continue;
      }
      this.register(face, bytes);
      loaded.push(face.id);
    }
    return { loaded, missing };
  }

  has(id: string): boolean {
    return this.byId.has(id) || this.instances.has(id);
  }

  /** The instances of face `id` resolved so far, by id. */
  instancesOf(id: string): string[] {
    return [...this.instances].filter(([, i]) => i.face.id === id).map(([k]) => k);
  }

  bytes(id: string): Uint8Array | undefined {
    return this.byId.get(id)?.bytes;
  }

  ids(): string[] {
    return this.faces.map((f) => f.id);
  }

  /** @deprecated Nothing uses it; it will be removed. */
  families(): string[] {
    return [...new Set(this.faces.map((f) => f.family))];
  }

  /** The `fonts[]` declaration of a registered id, or of an instance. */
  decl(id: string): FontDecl {
    const instance = this.instances.get(id);
    const f = instance?.face ?? this.byId.get(id);
    if (!f) throw new Error(`Font "${id}" is not registered`);
    const d: FontDecl = { id, family: f.family };
    const weight = instance?.weight ?? f.weight;
    if (weight !== 400) d.weight = weight;
    if (f.style !== 'normal') d.style = f.style;
    const stretch = instance?.stretch ?? f.stretch;
    if (stretch !== 100) d.stretch = stretch;
    if (instance) {
      d.source = f.id;
      d.variations = { ...instance.variations };
    } else if (Object.keys(f.variations).length > 0) {
      d.variations = { ...f.variations };
    }
    return d;
  }

  /**
   * The font for `families`, `weight`, `style` and width `stretch` (a percentage), as CSS matches
   * it; with `variations` (`fontVariationSettings`), or a weight or width within a variable face's
   * range, an instance of the face at those axis values. A `wght` or `wdth` in `variations` wins
   * over `weight` or `stretch` for the axis.
   */
  resolve(
    families: readonly string[],
    weight: number,
    style: 'normal' | 'italic',
    variations?: Readonly<Record<string, number>>,
    stretch = 100,
  ): ResolvedFont {
    const axes = variations ? byTag(variations) : [];
    const key = `${families.join(',')}|${weight}|${style}|${stretch}|${axesKey(axes)}`;
    let hit = this.memo.get(key);
    if (!hit) {
      hit = this.match(families, weight, style, stretch, axes);
      if (this.memo.size >= MAX_KEPT_FONTS) this.memo.clear();
      this.memo.set(key, hit);
    }
    return hit;
  }

  private match(
    families: readonly string[],
    weight: number,
    style: 'normal' | 'italic',
    stretch: number,
    axes: ReadonlyArray<readonly [string, number]>,
  ): ResolvedFont {
    if (this.faces.length === 0) throw new Error('No fonts are registered');
    const width = stretch === 100 ? '' : ` ${stretch}%`;
    const wanted = `${families[0] ?? '(none)'} ${weight}${width}${style === 'italic' ? ' italic' : ''}`;
    const index = families.findIndex((f) => this.faces.some((x) => sameFamily(x.family, f)));
    let family = families[index];
    let familyExact = index === 0;
    if (!family) {
      family = this.faces[0]?.family as string;
      familyExact = false;
    }
    const inFamily = this.faces.filter((f) => sameFamily(f.family, family as string));
    const widest = nearestStretch(inFamily, stretch);
    const sameStyle = widest.filter((f) => f.style === style);
    const pool = sameStyle.length > 0 ? sameStyle : widest;
    const face = nearestWeight(pool, weight);
    const drawn = clamp(weight, face.weights);
    const drawnWidth = clamp(stretch, face.widths);
    const exact = familyExact && sameStyle.length > 0 && drawn === weight && drawnWidth === stretch;
    const id = this.instance(face, drawn, drawnWidth, axes);
    return exact ? { id, exact } : { id, exact, wanted };
  }

  /**
   * The id of `face` at weight `weight` and width `stretch` with `axes`: the face's own for a
   * static face asked for no axes, else an instance's (`Inter-100-900@wght=650`), declared with
   * the face as its source. A face with a weight or width range always takes an instance: its
   * default `wght` and `wdth` are the font's to say.
   */
  private instance(
    face: Face,
    weight: number,
    stretch: number,
    axes: ReadonlyArray<readonly [string, number]>,
  ): string {
    const ranged = face.weights[0] !== face.weights[1];
    const wide = face.widths[0] !== face.widths[1];
    if (!ranged && !wide && axes.length === 0) return face.id;
    const variations: Record<string, number> = { ...face.variations };
    if (ranged) variations.wght = weight;
    if (wide) variations.wdth = stretch;
    for (const [tag, v] of axes) variations[tag] = v;
    const id = `${face.id}@${axesKey(byTag(variations))}`;
    const known = this.instances.get(id);
    if (known) {
      // Most recently used last.
      this.instances.delete(id);
      this.instances.set(id, known);
      return id;
    }
    if (this.instances.size >= MAX_KEPT_FONTS) {
      const oldest = this.instances.keys().next().value as string;
      this.instances.delete(oldest);
      // Fonts resolved before may name it.
      this.memo.clear();
    }
    const wght = variations.wght;
    const declared =
      wght !== undefined && Number.isFinite(wght) ? clamp(Math.round(wght), [1, 1000]) : weight;
    const wdth = variations.wdth;
    const width = wdth !== undefined && Number.isFinite(wdth) ? clamp(wdth, [1, 1000]) : stretch;
    this.instances.set(id, { face, weight: declared, stretch: width, variations });
    return id;
  }
}

/** `n` within `lo` and `hi`. */
const clamp = (n: number, [lo, hi]: readonly [number, number]): number => Math.min(Math.max(n, lo), hi);

/** Axis values ordered by tag, so that the same values make the same key. */
const byTag = (variations: Readonly<Record<string, number>>): Array<[string, number]> =>
  Object.entries(variations).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));

/** Axis values ordered by tag as an id takes them: `wdth=75,wght=650`. */
const axesKey = (axes: ReadonlyArray<readonly [string, number]>): string =>
  axes.map(([t, v]) => `${t}=${v}`).join(',');

/** What tells family names apart: they match ignoring ASCII case, as in CSS and in the engine. */
export const familyKey = (family: string): string => family.replace(/[A-Z]/g, (c) => c.toLowerCase());
const sameFamily = (a: string, b: string): boolean => a === b || familyKey(a) === familyKey(b);

/**
 * CSS Fonts 4 §5.2 width matching: the faces whose widths hold `w`, else those nearest it, for
 * 100% or less the narrower first, above it the wider first, each standing at the end of its
 * widths nearer `w`.
 */
function nearestStretch(pool: readonly Face[], w: number): Face[] {
  const at = (f: Face) => clamp(w, f.widths);
  let best: number | undefined;
  if (pool.some((f) => at(f) === w)) {
    best = w;
  } else {
    const narrower = pool.map(at).filter((a) => a < w);
    const wider = pool.map(at).filter((a) => a > w);
    const narrow = narrower.length > 0 ? Math.max(...narrower) : undefined;
    const wide = wider.length > 0 ? Math.min(...wider) : undefined;
    best = w <= 100 ? (narrow ?? wide) : (wide ?? narrow);
  }
  return pool.filter((f) => at(f) === best);
}

/**
 * CSS Fonts 4 §5.2 weight matching: a face whose weights hold `w`, else the nearest by the rules,
 * a variable face standing at the end of its range nearer `w`.
 */
function nearestWeight(pool: readonly Face[], w: number): Face {
  const exact = pool.find((f) => f.weights[0] <= w && w <= f.weights[1]);
  if (exact) return exact;
  // Where each face stands: the end of its range nearer `w`.
  const at = (f: Face) => (w < f.weights[0] ? f.weights[0] : f.weights[1]);
  const asc = [...pool].sort((a, b) => at(a) - at(b));
  const above = asc.filter((f) => at(f) > w);
  const below = asc.filter((f) => at(f) < w).reverse();
  let order: Face[];
  if (w >= 400 && w <= 500) {
    const upTo500 = above.filter((f) => at(f) <= 500);
    order = [...upTo500, ...below, ...above.filter((f) => at(f) > 500)];
  } else if (w < 400) {
    order = [...below, ...above];
  } else {
    order = [...above, ...below];
  }
  return order[0] as Face;
}
