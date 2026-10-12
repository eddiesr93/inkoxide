import { clampRgb, converter, parse as parseCssColor } from 'culori';

import type { Color } from './types.ts';

/** Points per CSS `rem`: `text-base` is 12 pt, `p-4` is 12 pt. */
export const PT_PER_REM = 12;
/** Points per CSS `px`: 1 px = 1/96 in = 0.75 pt. */
export const PT_PER_PX = 0.75;

export class ValueError extends Error {}

/** A number with a unit after conversion: `pt` for every absolute length. */
export interface Dim {
  v: number;
  u: 'pt' | '%' | 'em' | 'deg' | '';
}

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
 * Replaces every `var(--name, fallback)` with its value, recursively. `lookup` returns the value of
 * a custom property or undefined; an unresolvable `var()` without a fallback throws.
 */
export function substituteVars(
  value: string,
  lookup: (name: string) => string | undefined,
  depth = 0,
): string {
  if (depth > 16) throw new ValueError(`var() nesting too deep in "${value}"`);
  let i = value.indexOf('var(');
  if (i < 0) return value;
  let out = '';
  let last = 0;
  while (i >= 0) {
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
    out += value.slice(last, i) + substituteVars(resolved, lookup, depth + 1);
    last = close + 1;
    i = value.indexOf('var(', last);
  }
  return out + value.slice(last);
}

// ---------------------------------------------------------------------------------------------
// calc() evaluation

const UNIT_RE = /^(-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?)(px|rem|em|pt|mm|cm|in|pc|%|deg|turn|rad)?/i;

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

function toDim(n: number, unit: string | undefined): Dim {
  const u = (unit ?? '').toLowerCase();
  const pt = PT_PER_UNIT[u];
  if (pt !== undefined) return { v: n * pt, u: 'pt' };
  switch (u) {
    case '':
      return { v: n, u: '' };
    case 'em':
      return { v: n, u: 'em' };
    case '%':
      return { v: n, u: '%' };
    case 'deg':
      return { v: n, u: 'deg' };
    case 'turn':
      return { v: n * 360, u: 'deg' };
    case 'rad':
      return { v: (n * 180) / Math.PI, u: 'deg' };
    default:
      throw new ValueError(`unsupported unit "${unit}"`);
  }
}

class CalcParser {
  private i = 0;
  constructor(private readonly s: string) {}

  parse(): Dim {
    const d = this.expr();
    this.ws();
    if (this.i < this.s.length) throw new ValueError(`unexpected "${this.s.slice(this.i)}" in "${this.s}"`);
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
      const open = this.i + (rest.startsWith('calc(') ? 4 : 0);
      const close = matchParen(this.s, open);
      const inner = new CalcParser(this.s.slice(open + 1, close)).parse();
      this.i = close + 1;
      return inner;
    }
    if (rest.startsWith('infinity')) {
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
    return toDim(Number(m[1]), m[2]);
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

/** Evaluates a length-like CSS value (`0.25rem`, `calc(1 / 2 * 100%)`, `11pt`, `1.5`). */
export function evalDim(value: string): Dim {
  return new CalcParser(value.trim()).parse();
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

const NUMBER_RE = /^[-+]?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?$/i;

/**
 * Splits CSS `flex` into grow, shrink and basis: a keyword (`none`, `auto`, `initial`) or
 * `<grow> [<shrink>] [<basis>]`, where a second value that is not a number is the basis
 * (`1 auto`). A missing shrink is 1 and a missing basis `0%`. `number` reads grow and shrink,
 * `length` the basis.
 */
export function parseFlex<L>(
  value: string,
  number: (v: string) => number,
  length: (v: string) => L,
): [grow: number, shrink: number, basis: L | 'auto' | '0%'] {
  const v = value.trim();
  const keyword = Object.hasOwn(FLEX_KEYWORDS, v) ? FLEX_KEYWORDS[v] : undefined;
  if (keyword) return [...keyword];
  const [grow, second, basis] = splitTopLevel(v, ' ');
  if (grow === undefined) throw new ValueError(`flex "${value}" is not a number or "grow shrink basis"`);
  if (second !== undefined && basis === undefined && !NUMBER_RE.test(second)) {
    return [number(grow), 1, length(second)];
  }
  return [
    number(grow),
    second === undefined ? 1 : number(second),
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
    return [base[0], base[1], base[2], round(base[3] * pct)];
  }
  const parsed = parseCssColor(v);
  if (!parsed) throw new ValueError(`cannot parse colour "${v}"`);
  const rgb = clampRgb(toRgb(parsed));
  const c = (x: number | undefined): number => round(Math.min(1, Math.max(0, x ?? 0)));
  return [c(rgb.r), c(rgb.g), c(rgb.b), c(rgb.alpha ?? 1)];
}

/** Unquotes and splits a `font-family` stack. */
export function parseFontFamily(value: string): string[] {
  return splitTopLevel(value).map((f) => f.replace(/^["']|["']$/g, '').trim());
}
