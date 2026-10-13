// OpenType features: CSS `font-feature-settings` and `font-variant-numeric`, read into the
// engine's `fontFeatures` (tag → value).
import { UnsupportedValueError } from './reader.ts';
import { splitTopLevel } from './values.ts';

/** OpenType features by tag, as a run's `fontFeatures`: 0 turns one off, 1 on, more picks an alternate. */
export type FontFeatures = Readonly<Record<string, number>>;

/**
 * `fontFeatureSettings` as a style takes it: a CSS string (`"'tnum' 1, 'smcp'"`, `'normal'`), a
 * list of tags to turn on (`['tnum', 'smcp']`), or tags with a value or a switch
 * (`{ tnum: 1, liga: false }`).
 */
export type FontFeatureSettings = string | readonly string[] | Readonly<Record<string, number | boolean>>;

/** The schema's bounds: features a run takes, and the largest value of one. */
const MAX_FEATURES = 32;
const MAX_VALUE = 65_535;

const TAG = /^[\x20-\x7e]{4}$/;

function checked(prop: string, shown: string, features: Record<string, number>): FontFeatures {
  if (Object.keys(features).length > MAX_FEATURES) {
    throw new UnsupportedValueError(prop, `${prop}: ${shown} (at most ${MAX_FEATURES} features)`);
  }
  return features;
}

function tagOf(prop: string, shown: string, tag: string): string {
  if (!TAG.test(tag)) {
    throw new UnsupportedValueError(
      prop,
      `${prop}: ${shown} (${JSON.stringify(tag)} is not a feature tag: four ASCII characters, such as "tnum")`,
    );
  }
  return tag;
}

function valueOf(prop: string, shown: string, tag: string, v: number | boolean): number {
  const n = typeof v === 'boolean' ? Number(v) : v;
  if (!Number.isInteger(n) || n < 0 || n > MAX_VALUE) {
    throw new UnsupportedValueError(
      prop,
      `${prop}: ${shown} (the value of "${tag}" is a whole number from 0 to ${MAX_VALUE}, or on or off)`,
    );
  }
  return n;
}

/**
 * Reads `fontFeatureSettings` (CSS `font-feature-settings`) into the engine's `fontFeatures`. A
 * later setting of a tag wins; `'normal'` (or nothing) sets none.
 */
export function parseFontFeatureSettings(prop: string, value: FontFeatureSettings): FontFeatures {
  const shown = typeof value === 'string' ? value : JSON.stringify(value);
  const out: Record<string, number> = {};
  if (Array.isArray(value)) {
    for (const tag of value as readonly unknown[]) {
      if (typeof tag !== 'string')
        throw new UnsupportedValueError(prop, `${prop}: ${shown} (a list of tags)`);
      out[tagOf(prop, shown, tag)] = 1;
    }
    return checked(prop, shown, out);
  }
  if (typeof value === 'object' && value !== null) {
    for (const [tag, v] of Object.entries(value as Record<string, unknown>)) {
      if (typeof v !== 'number' && typeof v !== 'boolean') {
        throw new UnsupportedValueError(
          prop,
          `${prop}: ${shown} (the value of "${tag}" is a number or a boolean)`,
        );
      }
      out[tagOf(prop, shown, tag)] = valueOf(prop, shown, tag, v);
    }
    return checked(prop, shown, out);
  }
  if (typeof value !== 'string') {
    throw new UnsupportedValueError(prop, `${prop}: ${shown} (a CSS string, a list of tags or an object)`);
  }
  if (value.trim() === 'normal') return {};
  for (const item of splitFeatures(value)) {
    // `"tnum"`, `'tnum' 1`, `"smcp" on`.
    const m = /^(["'])(.*)\1(?:\s+(\S+))?$/s.exec(item);
    if (!m) {
      throw new UnsupportedValueError(prop, `${prop}: ${value} (expected a quoted tag such as "'tnum' 1")`);
    }
    const tag = tagOf(prop, value, m[2] ?? '');
    const v = m[3];
    const n = v === undefined || v === 'on' ? 1 : v === 'off' ? 0 : /^\d+$/.test(v) ? Number(v) : Number.NaN;
    out[tag] = valueOf(prop, value, tag, n);
  }
  return checked(prop, value, out);
}

/** Splits a feature list at the commas outside quotes; an empty item between commas stays, to fail. */
function splitFeatures(s: string): string[] {
  const out: string[] = [];
  let quote = '';
  let start = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s[i] ?? '';
    if (quote) {
      if (c === quote) quote = '';
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === ',') {
      out.push(s.slice(start, i));
      start = i + 1;
    }
  }
  out.push(s.slice(start));
  return out.map((x) => x.trim()).filter((x, i, all) => x.length > 0 || all.length > 1);
}

/** The `font-variant-numeric` keywords, each with its feature and the keywords it excludes. */
const NUMERIC: Readonly<Record<string, readonly [tag: string, group: string]>> = {
  ordinal: ['ordn', 'ordinal'],
  'slashed-zero': ['zero', 'zero'],
  'lining-nums': ['lnum', 'figure'],
  'oldstyle-nums': ['onum', 'figure'],
  'proportional-nums': ['pnum', 'spacing'],
  'tabular-nums': ['tnum', 'spacing'],
  'diagonal-fractions': ['frac', 'fraction'],
  'stacked-fractions': ['afrc', 'fraction'],
};

/**
 * Reads CSS `font-variant-numeric` (`'tabular-nums slashed-zero'`, `'normal'`) into the features
 * it turns on.
 */
export function parseFontVariantNumeric(prop: string, value: string): FontFeatures {
  if (typeof value !== 'string') {
    throw new UnsupportedValueError(
      prop,
      `${prop}: ${JSON.stringify(value)} (a CSS string such as 'tabular-nums')`,
    );
  }
  const words = splitTopLevel(value, ' ');
  if (words.length === 1 && words[0] === 'normal') return {};
  const out: Record<string, number> = {};
  const groups = new Set<string>();
  for (const w of words) {
    const known = Object.hasOwn(NUMERIC, w) ? NUMERIC[w] : undefined;
    if (!known) {
      throw new UnsupportedValueError(
        prop,
        `${prop}: ${value} (supported: normal, ${Object.keys(NUMERIC).join(', ')})`,
      );
    }
    const [tag, group] = known;
    if (groups.has(group)) {
      throw new UnsupportedValueError(prop, `${prop}: ${value} (two values of one kind)`);
    }
    groups.add(group);
    out[tag] = 1;
  }
  return out;
}
