import { clampRgb, converter, parse as parseCssColor } from 'culori';

import type { Color } from './types.ts';

/** Points per CSS `rem`: `text-base` is 12 pt, `p-4` is 12 pt. */
export const PT_PER_REM = 12;
/** Points per CSS `px`: 1 px = 1/96 in = 0.75 pt. */
export const PT_PER_PX = 0.75;

/**
 * A value that does not parse at all: `parseColor` and `parseFlex` throw it. Errors that name the
 * CSS property are `UnsupportedValueError`s.
 */
export class ValueError extends Error {}

/** A number with a unit after conversion: `pt` for every absolute length. */
export interface Dim {
  v: number;
  u: 'pt' | '%' | 'em' | 'deg' | '';
}

/** What the relative units of a length resolve against, in points. */
export interface Units {
  /** The font size, for `em`: the element's own, or the inherited one in `fontSize` itself. */
  em: number;
  /** The page size, for `vw` and `vh` (1% of the width and height). */
  page: { width: number; height: number };
}

/** Why `em`, `vw` or `vh` cannot be read without `Units`. */
export const UNITS_UNKNOWN = 'depends on the font or page size, which is not known here';

/** Finds the index of the `)` matching the `(` at `open`. */
function matchParen(s: string, open: number): number {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    const c = s[i];
    if (c === '(') depth++;
    else if (c === ')' && --depth === 0) return i;
  }
  throw new ValueError(`unbalanced parentheses in "${s}"`);
}

/**
 * Splits at `sep` outside parentheses (`a, rgb(0, 0, 0)`), trimming and dropping empty parts.
 * A `' '` separator splits at any whitespace (`1px solid rgb(0 0 0)`).
 */
export function splitTopLevel(s: string, sep = ','): string[] {
  const out: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s[i] ?? '';
    if (c === '(') depth++;
    else if (c === ')') depth--;
    else if (depth === 0 && (sep === ' ' ? /\s/.test(c) : c === sep)) {
      out.push(s.slice(start, i));
      start = i + 1;
    }
  }
  out.push(s.slice(start));
  return out.map((x) => x.trim()).filter((x) => x.length > 0);
}

/**
 * Most `var()`s one substitution may expand, and the longest value it may produce. Each `var()` is
 * expanded again wherever it is used, so a value that names one property several times, at each of
 * a few levels, grows exponentially; browsers bound it in the same way.
 */
const MAX_EXPANSIONS = 10_000;
const MAX_SUBSTITUTED = 65_536;

/**
 * Replaces every `var(--name, fallback)` with its value, recursively. `lookup` returns the value of
 * a custom property or undefined; an unresolvable `var()` without a fallback throws, as does a
 * substitution past {@link MAX_EXPANSIONS} `var()`s or {@link MAX_SUBSTITUTED} characters.
 */
export function substituteVars(
  value: string,
  lookup: (name: string) => string | undefined,
  depth = 0,
): string {
  return substitute(value, lookup, depth, { expansions: 0 });
}

function substitute(
  value: string,
  lookup: (name: string) => string | undefined,
  depth: number,
  budget: { expansions: number },
): string {
  if (depth > 16) throw new ValueError(`var() nesting too deep in "${value}"`);
  let i = value.indexOf('var(');
  if (i < 0) return value;
  let out = '';
  let last = 0;
  while (i >= 0) {
    if (++budget.expansions > MAX_EXPANSIONS) {
      throw new ValueError(`var() expands more than ${MAX_EXPANSIONS} times`);
    }
    const close = matchParen(value, i + 3);
    const inner = value.slice(i + 4, close);
    const comma = inner.indexOf(',');
    const name = (comma < 0 ? inner : inner.slice(0, comma)).trim();
    const fallback = comma < 0 ? undefined : inner.slice(comma + 1).trim();
    let resolved = lookup(name);
    if (resolved === undefined) {
      if (fallback === undefined) throw new ValueError(`cannot resolve var(${name})`);
      resolved = fallback;
    }
    out += value.slice(last, i) + substitute(resolved, lookup, depth + 1, budget);
    if (out.length > MAX_SUBSTITUTED) {
      throw new ValueError(`var() expands to more than ${MAX_SUBSTITUTED} characters`);
    }
    last = close + 1;
    i = value.indexOf('var(', last);
  }
  return out + value.slice(last);
}

// ---------------------------------------------------------------------------------------------
// calc() evaluation

/** A CSS `<number>` as regular expression source: the one grammar every value reader shares. */
export const NUMBER_SOURCE = String.raw`[-+]?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?`;
/** A string that is a number and nothing else. */
export const NUMBER_RE = new RegExp(`^${NUMBER_SOURCE}$`, 'i');

// Units in any case (`5PX`), and after a space (`5 pt`), as the `style` prop has always read them.
const UNIT_RE = new RegExp(
  `^(${NUMBER_SOURCE})(?:\\s*(px|rem|em|pt|mm|cm|in|pc|vw|vh|%|deg|turn|rad))?`,
  'i',
);

/** Points per absolute CSS unit. `rem` is the root font size Tailwind uses. */
export const PT_PER_UNIT: Readonly<Record<string, number>> = {
  pt: 1,
  mm: 72 / 25.4,
  cm: 720 / 25.4,
  in: 72,
  px: PT_PER_PX,
  pc: 12,
  rem: PT_PER_REM,
};

/** How `CalcParser` reads relative units: against `units`, and `em` kept as `em` with `keepEm`. */
interface Relative {
  units: Units | undefined;
  keepEm: boolean;
}

/** Why a bare `infinity` (`width: infinity`) is not a value. */
const INFINITY_OUTSIDE_CALC = 'infinity is only allowed inside calc()';

function toDim(n: number, unit: string | undefined, rel: Relative): Dim {
  const d = unitDim(n, (unit ?? '').toLowerCase(), rel);
  // A number past the float range (`1e999pt`): only `infinity` is infinite.
  if (!Number.isFinite(d.v)) throw new ValueError('too large a number');
  return d;
}

function unitDim(n: number, u: string, { units, keepEm }: Relative): Dim {
  const pt = PT_PER_UNIT[u];
  if (pt !== undefined) return { v: n * pt, u: 'pt' };
  switch (u) {
    case '':
      return { v: n, u: '' };
    case 'em':
      return units && !keepEm ? { v: n * units.em, u: 'pt' } : { v: n, u: 'em' };
    case 'vw':
    case 'vh':
      if (!units) throw new ValueError(UNITS_UNKNOWN);
      return { v: (n * (u === 'vw' ? units.page.width : units.page.height)) / 100, u: 'pt' };
    case '%':
      return { v: n, u: '%' };
    case 'deg':
      return { v: n, u: 'deg' };
    case 'turn':
      return { v: n * 360, u: 'deg' };
    case 'rad':
      return { v: (n * 180) / Math.PI, u: 'deg' };
    default:
      throw new ValueError(`unsupported unit "${u}"`);
  }
}

class CalcParser {
  private i = 0;
  private readonly s: string;
  private readonly rel: Relative;
  /** Inside `calc()`, where `infinity` is a number. */
  private readonly inCalc: boolean;
  constructor(s: string, rel: Relative, inCalc = false) {
    this.s = s;
    this.rel = rel;
    this.inCalc = inCalc;
  }

  parse(): Dim {
    const d = this.expr();
    this.ws();
    if (this.i < this.s.length) throw new ValueError(`unexpected "${this.s.slice(this.i)}" in "${this.s}"`);
    // `calc(0 / 0)`, `calc(infinity - infinity)`: not a number, which no property takes.
    if (Number.isNaN(d.v)) throw new ValueError(`"${this.s}" is not a number`);
    return d;
  }

  private ws(): void {
    while (this.i < this.s.length && /\s/.test(this.s[this.i] ?? '')) this.i++;
  }

  private expr(): Dim {
    let a = this.term();
    for (;;) {
      this.ws();
      const c = this.s[this.i];
      if (c !== '+' && c !== '-') return a;
      this.i++;
      const b = this.term();
      a = add(a, c === '+' ? b : { v: -b.v, u: b.u });
    }
  }

  private term(): Dim {
    let a = this.factor();
    for (;;) {
      this.ws();
      const c = this.s[this.i];
      if (c !== '*' && c !== '/') return a;
      this.i++;
      const b = this.factor();
      a = c === '*' ? mul(a, b) : div(a, b);
    }
  }

  private factor(): Dim {
    this.ws();
    const rest = this.s.slice(this.i);
    if (rest.startsWith('calc(') || rest.startsWith('(')) {
      const calc = rest.startsWith('calc(');
      const open = this.i + (calc ? 4 : 0);
      const close = matchParen(this.s, open);
      const inner = new CalcParser(this.s.slice(open + 1, close), this.rel, this.inCalc || calc).parse();
      this.i = close + 1;
      return inner;
    }
    if (rest.startsWith('infinity')) {
      if (!this.inCalc) throw new ValueError(INFINITY_OUTSIDE_CALC);
      this.i += 8;
      return { v: Number.POSITIVE_INFINITY, u: '' };
    }
    if (rest.startsWith('-') && !/^-[\d.]/.test(rest)) {
      this.i++;
      const f = this.factor();
      return { v: -f.v, u: f.u };
    }
    const m = UNIT_RE.exec(rest);
    if (!m || m[1] === undefined) throw new ValueError(`cannot evaluate "${this.s}"`);
    this.i += m[0].length;
    const unit = /^[a-z]+/i.exec(this.s.slice(this.i));
    if (unit) throw new ValueError(`unsupported unit "${m[2] ?? ''}${unit[0]}"`);
    return toDim(Number(m[1]), m[2], this.rel);
  }
}

function add(a: Dim, b: Dim): Dim {
  if (a.u === b.u) return { v: a.v + b.v, u: a.u };
  if (a.v === 0) return b;
  if (b.v === 0) return a;
  throw new ValueError(`cannot add ${a.u || 'a number'} and ${b.u || 'a number'}`);
}

function mul(a: Dim, b: Dim): Dim {
  if (a.u === '') return { v: a.v * b.v, u: b.u };
  if (b.u === '') return { v: a.v * b.v, u: a.u };
  throw new ValueError(`cannot multiply ${a.u} by ${b.u}`);
}

function div(a: Dim, b: Dim): Dim {
  if (b.u !== '') throw new ValueError(`cannot divide by ${b.u}`);
  return { v: a.v / b.v, u: a.u };
}

/**
 * Evaluates a length-like CSS value (`0.25rem`, `calc(1 / 2 * 100%)`, `11pt`, `1.5`). With
 * `units`, `vw`, `vh` and (unless `keepEm`) `em` become points; without, `em` stays `em` and `vw`
 * and `vh` throw.
 */
export function evalDim(value: string, units?: Units, keepEm = false): Dim {
  return new CalcParser(value.trim(), { units, keepEm }).parse();
}

/** Rounds away float noise (`0.25rem * 3` = 9.000000000000002 pt) so snapshots stay readable. */
export function round(n: number): number {
  return Math.round(n * 1e6) / 1e6;
}

// ---------------------------------------------------------------------------------------------
// Shorthands

/** The CSS `flex` keywords, as grow, shrink and basis. */
const FLEX_KEYWORDS: Readonly<Record<string, readonly [number, number, 'auto']>> = {
  none: [0, 0, 'auto'],
  auto: [1, 1, 'auto'],
  initial: [0, 1, 'auto'],
};

/**
 * Splits CSS `flex` into grow, shrink and basis, as CSS reads it: a keyword (`none`, `auto`,
 * `initial`), or `<grow> [<shrink>]` and a basis before or after them. The basis is the value
 * that is not a number (`30%`, `1 auto`), or the third of three numbers (`1 1 0`). A missing grow
 * or shrink is 1 and a missing basis `0%`, so `flex: 30%` is `1 1 30%`. `number` reads grow and
 * shrink, which must be plain numbers, and `length` the basis.
 */
export function parseFlex<L>(
  value: string,
  number: (v: string) => number,
  length: (v: string) => L,
): [grow: number, shrink: number, basis: L | 'auto' | '0%'] {
  const v = value.trim();
  const keyword = Object.hasOwn(FLEX_KEYWORDS, v) ? FLEX_KEYWORDS[v] : undefined;
  if (keyword) return [...keyword];
  const parts = splitTopLevel(v, ' ');
  const first = parts[0];
  const last = parts.at(-1);
  if (first === undefined || last === undefined || parts.length > 3) {
    throw new ValueError(`flex "${value}" is not a number or "grow shrink basis"`);
  }
  let basis: string | undefined;
  if (!NUMBER_RE.test(first)) basis = parts.shift();
  else if (parts.length === 3 || !NUMBER_RE.test(last)) basis = parts.pop();
  const [grow, shrink] = parts;
  const factor = (f: string): number => {
    if (!NUMBER_RE.test(f)) throw new ValueError(`flex "${value}": grow and shrink are numbers`);
    const n = number(f);
    if (n < 0) throw new ValueError(`flex "${value}": grow and shrink cannot be negative`);
    return n;
  };
  return [
    grow === undefined ? 1 : factor(grow),
    shrink === undefined ? 1 : factor(shrink),
    basis === undefined ? '0%' : length(basis),
  ];
}

// ---------------------------------------------------------------------------------------------
// Colours

const toRgb = converter('rgb');

/**
 * Parses a CSS colour (hex, rgb(), oklch(), named, `transparent`, and Tailwind's
 * `color-mix(in oklab, <colour> N%, transparent)` opacity modifier) into RGBA floats, or
 * `'currentColor'`.
 */
export function parseColor(value: string): Color | 'currentColor' {
  const v = value.trim();
  if (/^currentcolor$/i.test(v)) return 'currentColor';
  const mix = /^color-mix\(\s*in\s+[a-z-]+\s*,(.*)\)$/i.exec(v);
  if (mix?.[1] !== undefined) {
    const parts = splitTopLevel(mix[1]);
    const [first, second] = parts;
    if (parts.length !== 2 || first === undefined || second === undefined || !/^transparent$/i.test(second)) {
      throw new ValueError(`unsupported color-mix(): "${v}"`);
    }
    const m = /^(.*?)\s+(-?[\d.]+)%$/.exec(first);
    const base = parseColor(m?.[1] ?? first);
    if (base === 'currentColor') throw new ValueError('color-mix() with currentColor is not supported');
    const pct = m?.[2] !== undefined ? Number(m[2]) / 100 : 0.5;
    if (!(pct >= 0 && pct <= 1)) throw new ValueError(`color-mix(): "${v}" (a percentage from 0% to 100%)`);
    return [base[0], base[1], base[2], round(base[3] * pct)];
  }
  // Colour keywords are case-insensitive, as in CSS; culori reads `transparent` only in lower case.
  const parsed = parseCssColor(/^[a-z]+$/i.test(v) ? v.toLowerCase() : v);
  if (!parsed) throw new ValueError(`cannot parse colour "${v}"`);
  const rgb = clampRgb(toRgb(parsed));
  const c = (x: number | undefined): number => round(Math.min(1, Math.max(0, x ?? 0)));
  return [c(rgb.r), c(rgb.g), c(rgb.b), c(rgb.alpha ?? 1)];
}

/** The font stack of text that sets no family: Tailwind's `font-sans` in the default theme. */
export const DEFAULT_SANS: readonly string[] = ['Inter', 'Noto Sans'];

/** Unquotes and splits a `font-family` stack. */
export function parseFontFamily(value: string): string[] {
  return splitTopLevel(value).map((f) => f.replace(/^["']|["']$/g, '').trim());
}
