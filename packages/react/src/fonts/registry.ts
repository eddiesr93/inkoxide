import { readFileSync } from 'node:fs';
import { join } from 'node:path';

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

export interface FontFace {
  /** The engine font id, used in `fonts[].id` and `runs[].font` (`Inter-Bold`). */
  id: string;
  family: string;
  weight?: number;
  style?: 'normal' | 'italic';
  /**
   * File name, for `loadDirectory`.
   * @deprecated `loadDirectory` is deprecated: read the file and pass its bytes to `register`.
   */
  file?: string;
}

export interface ResolvedFont {
  id: string;
  /** False when the weight, style or family had to fall back. */
  exact: boolean;
  /** What was asked for, when it did not match exactly. */
  wanted?: string;
}

interface Face extends Required<Omit<FontFace, 'file'>> {
  bytes: Uint8Array | undefined;
}

/**
 * Maps family + weight + style to registered font ids, and holds the font bytes until the
 * engine registers them. Matching follows the CSS Fonts 4 algorithm: the first
 * registered family of the stack (ignoring ASCII case), then the style, then the nearest weight. The engine's
 * `FontSet::select` matches the same way; `fixtures/fonts/matching.json` holds the cases both
 * are tested with.
 */
export class FontRegistry {
  private readonly faces: Face[] = [];
  private readonly byId = new Map<string, Face>();
  private readonly memo = new Map<string, ResolvedFont>();
  /** Splits words into syllables in every `Text` without its own `hyphenationCallback`. */
  hyphenationCallback: HyphenationCallback | undefined;
  /** Draws the emoji of every `Text` as images from here. */
  emojiSource: EmojiSource | undefined;

  register(face: FontFace, bytes?: Uint8Array): this {
    if (this.byId.has(face.id)) throw new Error(`Font id "${face.id}" is already registered`);
    const f: Face = {
      id: face.id,
      family: face.family,
      weight: face.weight ?? 400,
      style: face.style ?? 'normal',
      bytes,
    };
    this.faces.push(f);
    this.byId.set(f.id, f);
    this.memo.clear();
    return this;
  }

  /**
   * Registers every face whose `file` is in `dir`, skipping ids already registered, and returns
   * the ids it registered (`loaded`) and the ids whose file it could not read (`missing`).
   * @deprecated Read the files and pass their bytes to `register`.
   */
  loadDirectory(dir: string, faces: readonly FontFace[]): { loaded: string[]; missing: string[] } {
    const loaded: string[] = [];
    const missing: string[] = [];
    for (const face of faces) {
      if (!face.file || this.byId.has(face.id)) continue;
      let bytes: Uint8Array;
      try {
        bytes = readFileSync(join(dir, face.file));
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
    return this.byId.has(id);
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

  /** The `fonts[]` declaration of a registered id. */
  decl(id: string): FontDecl {
    const f = this.byId.get(id);
    if (!f) throw new Error(`Font "${id}" is not registered`);
    const d: FontDecl = { id: f.id, family: f.family };
    if (f.weight !== 400) d.weight = f.weight;
    if (f.style !== 'normal') d.style = f.style;
    return d;
  }

  resolve(families: readonly string[], weight: number, style: 'normal' | 'italic'): ResolvedFont {
    const key = `${families.join(',')}|${weight}|${style}`;
    let hit = this.memo.get(key);
    if (!hit) {
      hit = this.match(families, weight, style);
      this.memo.set(key, hit);
    }
    return hit;
  }

  private match(families: readonly string[], weight: number, style: 'normal' | 'italic'): ResolvedFont {
    if (this.faces.length === 0) throw new Error('No fonts are registered');
    const wanted = `${families[0] ?? '(none)'} ${weight}${style === 'italic' ? ' italic' : ''}`;
    const index = families.findIndex((f) => this.faces.some((x) => sameFamily(x.family, f)));
    let family = families[index];
    let familyExact = index === 0;
    if (!family) {
      family = this.faces[0]?.family as string;
      familyExact = false;
    }
    const inFamily = this.faces.filter((f) => sameFamily(f.family, family as string));
    const sameStyle = inFamily.filter((f) => f.style === style);
    const pool = sameStyle.length > 0 ? sameStyle : inFamily;
    const face = nearestWeight(pool, weight);
    const exact = familyExact && sameStyle.length > 0 && face.weight === weight;
    return exact ? { id: face.id, exact } : { id: face.id, exact, wanted };
  }
}

/** Family names match ignoring ASCII case, as in CSS. */
const asciiLower = (s: string): string => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
const sameFamily = (a: string, b: string): boolean => a === b || asciiLower(a) === asciiLower(b);

/** CSS Fonts 4 §5.2 weight matching. */
function nearestWeight(pool: readonly Face[], w: number): Face {
  const exact = pool.find((f) => f.weight === w);
  if (exact) return exact;
  const asc = [...pool].sort((a, b) => a.weight - b.weight);
  const above = asc.filter((f) => f.weight > w);
  const below = asc.filter((f) => f.weight < w).reverse();
  let order: Face[];
  if (w >= 400 && w <= 500) {
    const upTo500 = above.filter((f) => f.weight <= 500);
    order = [...upTo500, ...below, ...above.filter((f) => f.weight > 500)];
  } else if (w < 400) {
    order = [...below, ...above];
  } else {
    order = [...above, ...below];
  }
  return order[0] as Face;
}
