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

/** Throws that a setting is unsupported, saying why. */
type Fail = (why: string) => never;

/** The {@link Fail} of the setting `shown` of style property `prop`. */
function failing(prop: string, shown: string): Fail {
  return (why) => {
    throw new UnsupportedValueError(prop, `${prop}: ${shown} (${why})`);
  };
}

function checked(fail: Fail, features: Record<string, number>): FontFeatures {
  if (Object.keys(features).length > MAX_FEATURES) fail(`at most ${MAX_FEATURES} features`);
  return features;
}

function tagOf(fail: Fail, tag: string): string {
  if (!TAG.test(tag)) {
    fail(`${JSON.stringify(tag)} is not a feature tag: four ASCII characters, such as "tnum"`);
  }
  return tag;
}

function valueOf(fail: Fail, tag: string, v: number | boolean): number {
  const n = typeof v === 'boolean' ? Number(v) : v;
  if (!Number.isInteger(n) || n < 0 || n > MAX_VALUE) {
    fail(`the value of "${tag}" is a whole number from 0 to ${MAX_VALUE}, or on or off`);
  }
  return n;
}

/**
 * Reads `fontFeatureSettings` (CSS `font-feature-settings`) into the engine's `fontFeatures`. A
 * later setting of a tag wins; `'normal'` (or nothing) sets none.
 */
export function parseFontFeatureSettings(prop: string, value: FontFeatureSettings): FontFeatures {
  const fail: Fail = failing(prop, typeof value === 'string' ? value : JSON.stringify(value));
  const out: Record<string, number> = {};
  if (Array.isArray(value)) {
    for (const tag of value as readonly unknown[]) {
      if (typeof tag !== 'string') fail('a list of tags');
      out[tagOf(fail, tag)] = 1;
    }
    return checked(fail, out);
  }
  if (typeof value === 'object' && value !== null) {
    for (const [tag, v] of Object.entries(value as Record<string, unknown>)) {
      if (typeof v !== 'number' && typeof v !== 'boolean') {
        fail(`the value of "${tag}" is a number or a boolean`);
      }
      out[tagOf(fail, tag)] = valueOf(fail, tag, v);
    }
    return checked(fail, out);
  }
  if (typeof value !== 'string') fail('a CSS string, a list of tags or an object');
  if (value.trim() === 'normal') return {};
  for (const item of splitFeatures(value)) {
    // `"tnum"`, `'tnum' 1`, `"smcp" on`.
    const m = /^(["'])(.*)\1(?:\s+(\S+))?$/s.exec(item);
    if (!m) fail(`expected a quoted tag such as "'tnum' 1"`);
    const tag = tagOf(fail, m[2] ?? '');
    const v = m[3];
    const n = v === undefined || v === 'on' ? 1 : v === 'off' ? 0 : /^\d+$/.test(v) ? Number(v) : Number.NaN;
    out[tag] = valueOf(fail, tag, n);
  }
  return checked(fail, out);
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
  if (typeof value !== 'string') failing(prop, JSON.stringify(value))("a CSS string such as 'tabular-nums'");
  const fail: Fail = failing(prop, value);
  const words = splitTopLevel(value, ' ');
  if (words.length === 1 && words[0] === 'normal') return {};
  const out: Record<string, number> = {};
  const groups = new Set<string>();
  for (const w of words) {
    const known = Object.hasOwn(NUMERIC, w) ? NUMERIC[w] : undefined;
    if (!known) fail(`supported: normal, ${Object.keys(NUMERIC).join(', ')}`);
    const [tag, group] = known;
    if (groups.has(group)) fail('two values of one kind');
    groups.add(group);
    out[tag] = 1;
  }
  return out;
}
