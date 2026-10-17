// Media queries for the page size: the subset of Media Queries 4 that has a meaning on a page of
// paper. A query is parsed once and matched against each page size; matching is a pure function
// of the query and the size, so output stays deterministic.

import { PT_PER_UNIT, ValueError } from './values.ts';

/** The size of a page, in points: its whole sheet, margins included. */
export interface PageSize {
  width: number;
  height: number;
}

/** A parsed media query list; it matches a page when any of its queries does. */
export interface MediaQuery {
  /** The query as written. */
  readonly source: string;
  matches(page: PageSize): boolean;
}

/** Longest query read: a class string, and so a query, may come from data. */
export const MAX_MEDIA_QUERY = 1000;

/** Features of screens and devices: named in their error, as having no meaning on paper. */
const SCREEN_FEATURES = new Set([
  'any-hover',
  'any-pointer',
  'color',
  'color-gamut',
  'color-index',
  'device-aspect-ratio',
  'device-height',
  'device-width',
  'display-mode',
  'dynamic-range',
  'forced-colors',
  'grid',
  'hover',
  'inverted-colors',
  'monochrome',
  'overflow-block',
  'overflow-inline',
  'pointer',
  'prefers-color-scheme',
  'prefers-contrast',
  'prefers-reduced-data',
  'prefers-reduced-motion',
  'prefers-reduced-transparency',
  'resolution',
  'scan',
  'scripting',
  'update',
  'video-dynamic-range',
]);

type Op = '<' | '<=' | '>' | '>=' | '=';

type Token =
  | { t: 'ident'; v: string }
  | { t: 'num'; v: number; unit: string }
  | { t: 'op'; v: Op }
  | { t: '(' | ')' | ':' | ',' | '/' };

type Test = (page: PageSize) => boolean;

const NUMBER = /[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?/iy;
const UNIT = /[a-z%]+/iy;
const IDENT = /-?[a-z_][\w-]*/iy;

function tokenize(s: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i] ?? '';
    if (/\s/.test(c)) {
      i++;
    } else if (c === '(' || c === ')' || c === ':' || c === ',' || c === '/') {
      out.push({ t: c });
      i++;
    } else if (c === '<' || c === '>' || c === '=') {
      const eq = c !== '=' && s[i + 1] === '=';
      out.push({ t: 'op', v: (eq ? `${c}=` : c) as Op });
      i += eq ? 2 : 1;
    } else if (/[\d.+]/.test(c) || (c === '-' && /[\d.]/.test(s[i + 1] ?? ''))) {
      NUMBER.lastIndex = i;
      const n = NUMBER.exec(s);
      if (!n) throw new ValueError(`"${c}" is not a number`);
      i += n[0].length;
      UNIT.lastIndex = i;
      const u = UNIT.exec(s);
      const unit = u ? u[0].toLowerCase() : '';
      i += unit.length;
      out.push({ t: 'num', v: Number(n[0]), unit });
    } else {
      IDENT.lastIndex = i;
      const id = IDENT.exec(s);
      if (!id) throw new ValueError(`unexpected "${c}"`);
      out.push({ t: 'ident', v: id[0].toLowerCase() });
      i += id[0].length;
    }
  }
  return out;
}

/** Rounds to 1/100 pt, the precision of the page presets: `210mm` is A4's 595.28pt. */
const round = (v: number): number => Math.round(v * 100) / 100;

function compare(a: number, op: Op, b: number): boolean {
  switch (op) {
    case '<':
      return a < b;
    case '<=':
      return a <= b;
    case '>':
      return a > b;
    case '>=':
      return a >= b;
    case '=':
      return a === b;
  }
}

const FLIP: Record<Op, Op> = { '<': '>', '<=': '>=', '>': '<', '>=': '<=', '=': '=' };

/** A value a feature is compared with: a length in points, or a ratio. */
type Value = { length: number } | { ratio: [number, number] };

class Parser {
  private i = 0;
  constructor(private readonly tokens: Token[]) {}

  private peek(): Token | undefined {
    return this.tokens[this.i];
  }

  private next(): Token {
    const t = this.tokens[this.i++];
    if (!t) throw new ValueError('the query ends too early');
    return t;
  }

  private expect(t: Token['t']): void {
    const got = this.next();
    if (got.t !== t) throw new ValueError(`expected "${t}", found ${show(got)}`);
  }

  private word(w: string): boolean {
    const t = this.peek();
    if (t?.t === 'ident' && t.v === w) {
      this.i++;
      return true;
    }
    return false;
  }

  /** `query (, query)*`. */
  list(): Test {
    const queries = [this.query()];
    while (this.peek()?.t === ',') {
      this.i++;
      queries.push(this.query());
    }
    const extra = this.peek();
    if (extra) throw new ValueError(`unexpected ${show(extra)}`);
    return (p) => queries.some((q) => q(p));
  }

  /** `[not|only]? type [and condition]?`, or a condition. */
  private query(): Test {
    const t = this.peek();
    if (t?.t === 'ident' && t.v !== 'not') {
      // `only` hid queries from old browsers; it changes nothing.
      this.word('only');
      const type = this.mediaType();
      const rest = this.word('and') ? this.condition(false) : undefined;
      return rest ? (p) => type && rest(p) : () => type;
    }
    if (t?.t === 'ident' && t.v === 'not') {
      const after = this.tokens[this.i + 1];
      if (after?.t === 'ident') {
        this.i++;
        const type = this.mediaType();
        const rest = this.word('and') ? this.condition(false) : undefined;
        return rest ? (p) => !(type && rest(p)) : () => !type;
      }
    }
    return this.condition(true);
  }

  private mediaType(): boolean {
    const t = this.next();
    if (t.t !== 'ident') throw new ValueError(`expected a media type, found ${show(t)}`);
    switch (t.v) {
      case 'all':
      case 'print':
        return true;
      case 'screen':
        return false;
      default:
        throw new ValueError(`unknown media type "${t.v}" (all, print or screen)`);
    }
  }

  /** `not (…)`, or `(…)` joined by `and`, or by `or` when `or` is allowed. */
  private condition(orAllowed: boolean): Test {
    if (this.word('not')) {
      const inner = this.inParens();
      return (p) => !inner(p);
    }
    const parts = [this.inParens()];
    let joiner: 'and' | 'or' | undefined;
    for (;;) {
      const t = this.peek();
      if (t?.t !== 'ident' || (t.v !== 'and' && t.v !== 'or')) break;
      if (t.v === 'or' && !orAllowed) throw new ValueError('"or" cannot follow a media type');
      if (joiner && joiner !== t.v) {
        throw new ValueError('"and" and "or" cannot be mixed without parentheses');
      }
      joiner = t.v;
      this.i++;
      parts.push(this.inParens());
    }
    if (parts.length === 1) return parts[0] as Test;
    return joiner === 'or' ? (p) => parts.some((q) => q(p)) : (p) => parts.every((q) => q(p));
  }

  /** `( condition )` or `( feature )`. */
  private inParens(): Test {
    this.expect('(');
    const t = this.peek();
    const nested = t?.t === '(' || (t?.t === 'ident' && t.v === 'not');
    const test = nested ? this.condition(true) : this.feature();
    this.expect(')');
    return test;
  }

  /** `name`, `name: value`, or a range: `name op value`, `value op name`, `v op name op v`. */
  private feature(): Test {
    const first = this.next();
    if (first.t === 'ident') {
      const name = first.v;
      const t = this.peek();
      if (t?.t === ':') {
        this.i++;
        return plain(name, this);
      }
      if (t?.t === 'op') {
        this.i++;
        const value = this.value();
        return range(name, t.v, value);
      }
      return boolean(name);
    }
    // `value op name [op value]`.
    this.i--;
    const low = this.value();
    const op1 = this.next();
    if (op1.t !== 'op') throw new ValueError(`expected a comparison, found ${show(op1)}`);
    const nameTok = this.next();
    if (nameTok.t !== 'ident') throw new ValueError(`expected a media feature, found ${show(nameTok)}`);
    const name = nameTok.v;
    const lowTest = range(name, FLIP[op1.v], low);
    const op2 = this.peek();
    if (op2?.t !== 'op') return lowTest;
    this.i++;
    const sameWay = (a: Op, b: Op) => (a[0] === '<' && b[0] === '<') || (a[0] === '>' && b[0] === '>');
    if (!sameWay(op1.v, op2.v)) throw new ValueError('a range takes two "<" or two ">" comparisons');
    const highTest = range(name, op2.v, this.value());
    return (p) => lowTest(p) && highTest(p);
  }

  /** A length (`48rem`, `210mm`, `0`), a ratio (`16/9`) or a bare number. */
  value(): Value {
    const t = this.next();
    if (t.t !== 'num') throw new ValueError(`expected a value, found ${show(t)}`);
    if (this.peek()?.t === '/') {
      this.i++;
      const d = this.next();
      if (d.t !== 'num' || d.unit !== '' || t.unit !== '')
        throw new ValueError('a ratio is two numbers, like 16/9');
      return { ratio: [t.v, d.v] };
    }
    if (t.unit === '') return t.v === 0 ? { length: 0 } : { ratio: [t.v, 1] };
    const pt = PT_PER_UNIT[t.unit];
    if (pt === undefined) {
      if (['em', 'vw', 'vh', '%', 'ex', 'ch', 'vmin', 'vmax'].includes(t.unit)) {
        throw new ValueError(
          `"${t.v}${t.unit}" has no fixed size in a media query; use pt, mm, cm, in, px or rem`,
        );
      }
      throw new ValueError(`unsupported unit "${t.unit}"`);
    }
    return { length: t.v * pt };
  }

  /** An identifier value (`portrait`). */
  ident(): string {
    const t = this.next();
    if (t.t !== 'ident') throw new ValueError(`expected a keyword, found ${show(t)}`);
    return t.v;
  }
}

function show(t: Token): string {
  switch (t.t) {
    case 'ident':
      return `"${t.v}"`;
    case 'num':
      return `"${t.v}${t.unit}"`;
    case 'op':
      return `"${t.v}"`;
    default:
      return `"${t.t}"`;
  }
}

/** Throws for a feature the page has not. */
function known(name: string): void {
  if (SCREEN_FEATURES.has(name) || SCREEN_FEATURES.has(name.replace(/^(min|max)-/, ''))) {
    throw new ValueError(`"${name}" has no meaning in a PDF`);
  }
  throw new ValueError(
    `unknown media feature "${name}" (a page has width, height, aspect-ratio and orientation)`,
  );
}

/** The side of the page `name` measures. */
function side(name: string): 'width' | 'height' | 'aspect-ratio' | undefined {
  return name === 'width' || name === 'height' || name === 'aspect-ratio' ? name : undefined;
}

/** `name op value`, the page's measure on the left. */
function range(name: string, op: Op, value: Value): Test {
  const s = side(name);
  if (!s) {
    if (name === 'orientation') throw new ValueError('orientation takes portrait or landscape, not a range');
    return known(name) as never;
  }
  if (s === 'aspect-ratio') {
    if (!('ratio' in value)) throw new ValueError('aspect-ratio takes a ratio, like 16/9');
    const [n, d] = value.ratio;
    if (!(n > 0 && d > 0)) throw new ValueError('a ratio takes two positive numbers');
    return (p) => compare(round(p.width) * d, op, round(p.height) * n);
  }
  if (!('length' in value)) {
    throw new ValueError(`${s} takes a length, like 210mm`);
  }
  const v = round(value.length);
  return (p) => compare(round(p[s]), op, v);
}

/** `name: value`: `min-` and `max-` prefixes, and orientation. */
function plain(name: string, parser: Parser): Test {
  if (name === 'orientation') {
    const v = parser.ident();
    if (v !== 'portrait' && v !== 'landscape') {
      throw new ValueError(`orientation is portrait or landscape, not "${v}"`);
    }
    // A square page is portrait.
    return v === 'portrait' ? (p) => p.height >= p.width : (p) => p.width > p.height;
  }
  const prefix = /^(min|max)-/.exec(name)?.[1];
  const base = prefix ? name.slice(prefix.length + 1) : name;
  if (!side(base)) return known(name) as never;
  return range(base, prefix === 'min' ? '>=' : prefix === 'max' ? '<=' : '=', parser.value());
}

/** `(name)`: true when the feature is not zero, which every page's is. */
function boolean(name: string): Test {
  if (side(name) || name === 'orientation') return () => true;
  return known(name) as never;
}

/**
 * Parses a media query list (`(orientation: landscape)`, `print and (width >= 48rem)`). Throws a
 * `ValueError` that quotes it when it does not parse or names a feature a page has not.
 */
export function parseMedia(source: string): MediaQuery {
  const fail = (why: string): never => {
    throw new ValueError(`@media ${source.length > 80 ? `${source.slice(0, 80)}…` : source}: ${why}`);
  };
  if (source.length > MAX_MEDIA_QUERY) fail(`a query is at most ${MAX_MEDIA_QUERY} characters`);
  if (source.trim() === '') fail('the query is empty');
  let test: Test;
  try {
    test = new Parser(tokenize(source)).list();
  } catch (e) {
    if (e instanceof ValueError) return fail(e.message);
    throw e;
  }
  return { source, matches: test };
}
