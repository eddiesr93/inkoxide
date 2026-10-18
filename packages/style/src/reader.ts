// Reading CSS values into Inkoxide style values, one property at a time.
import type { GridPlacement, Style, TemplateTrack, TrackBreadth, TrackSize } from '@inkoxide/schema';

import { MAX_GRID_LINE, MAX_GRID_SPAN } from './bounds.ts';
import {
  BORDER_STYLES,
  type ColorValue,
  type DecorationStyle,
  type Len,
  type LetterSpacing,
  type LineHeight,
  type LenPercent,
  OBJECT_FITS,
  type ObjectFit,
  type ObjectPosition,
  type ShapeOutside,
  type TextDecoration,
  type TextTransform,
  type TextWrap,
  type TransformOp,
} from './types.ts';
import {
  type Dim,
  evalDim,
  parseColor,
  round,
  splitTopLevel,
  type Units,
  UNITS_UNKNOWN,
  ValueError,
} from './values.ts';

/** The values `ENUMS` may list for each property: the schema's, or the tree builder's. */
type EnumValues = { [K in keyof Style]?: readonly NonNullable<Style[K]>[] } & {
  decorationStyle: readonly DecorationStyle[];
  textTransform: readonly TextTransform[];
  objectFit: readonly ObjectFit[];
};

/** The keywords each enumerated property accepts. */
export const ENUMS = {
  display: ['flex', 'grid', 'none'],
  flexDirection: ['row', 'column', 'row-reverse', 'column-reverse'],
  flexWrap: ['nowrap', 'wrap', 'wrap-reverse'],
  justifyContent: [
    'flex-start',
    'flex-end',
    'center',
    'space-between',
    'space-around',
    'space-evenly',
    'start',
    'end',
    'stretch',
  ],
  alignItems: ['flex-start', 'flex-end', 'center', 'baseline', 'stretch', 'start', 'end'],
  alignSelf: ['auto', 'flex-start', 'flex-end', 'center', 'baseline', 'stretch', 'start', 'end'],
  alignContent: [
    'flex-start',
    'flex-end',
    'center',
    'space-between',
    'space-around',
    'space-evenly',
    'stretch',
    'start',
    'end',
  ],
  justifyItems: ['flex-start', 'flex-end', 'center', 'baseline', 'stretch', 'start', 'end'],
  justifySelf: ['auto', 'flex-start', 'flex-end', 'center', 'baseline', 'stretch', 'start', 'end'],
  position: ['relative', 'absolute'],
  overflow: ['visible', 'hidden'],
  float: ['left', 'right', 'none'],
  clear: ['left', 'right', 'both', 'none'],
  borderStyle: BORDER_STYLES,
  gridAutoFlow: ['row', 'column', 'row dense', 'column dense'],
  whiteSpace: ['normal', 'nowrap', 'pre', 'pre-wrap', 'pre-line'],
  textWrap: ['wrap', 'pretty', 'balance'],
  decorationStyle: ['solid', 'double', 'dotted', 'dashed', 'wavy'],
  direction: ['ltr', 'rtl'],
  textTransform: ['none', 'uppercase', 'lowercase', 'capitalize'],
  objectFit: OBJECT_FITS,
} as const satisfies EnumValues;

export type EnumKey = keyof typeof ENUMS;

/**
 * A CSS value the engine cannot express. `parseTransform`, `parseOrigin` and
 * `parseObjectPosition` throw it; `tw()` reports it as an `UnsupportedUtilityError`.
 */
export class UnsupportedValueError extends Error {
  override readonly name = 'UnsupportedValueError';
  constructor(
    /** The CSS property (or style key) whose value is not supported. */
    readonly property: string,
    /** What is wrong, usually `<property>: <value> (<why>)`. */
    readonly detail: string,
  ) {
    super(detail);
  }
}

/**
 * The length `calc(infinity * 1px)` (`rounded-full`) is read as, either sign, and the largest
 * radius.
 */
const INFINITE_PT = 1e4;

/**
 * Reads the values of one CSS property: its errors name the property. Lengths take `em`, `vw` and
 * `vh` against `units`; without them, those lengths are errors.
 */
export class Reader {
  constructor(
    readonly prop: string,
    readonly units?: Units,
  ) {}

  /** The error for a value of this property, to throw. */
  error(detail: string): UnsupportedValueError {
    return new UnsupportedValueError(this.prop, detail);
  }

  /** The value, when it is one of the keywords `ENUMS[key]` lists. */
  oneOf<K extends EnumKey>(key: K, value: string): (typeof ENUMS)[K][number] {
    const v = value.trim() as (typeof ENUMS)[K][number];
    if (!(ENUMS[key] as readonly string[]).includes(v)) {
      throw this.error(`${this.prop}: ${value} (supported: ${ENUMS[key].join(', ')})`);
    }
    return v;
  }

  /** A `text-wrap` mode. */
  textWrap(value: string): TextWrap {
    return this.oneOf('textWrap', value);
  }

  private dimToLen(d: Dim, value: string): Len {
    if ((d.u === 'pt' || d.u === '') && Math.abs(d.v) === Number.POSITIVE_INFINITY)
      return Math.sign(d.v) * INFINITE_PT;
    if (d.u === 'pt') return round(d.v);
    if (d.u === '%') return `${round(d.v)}%`;
    if (d.u === '' && d.v === 0) return 0;
    if (d.u === 'em') throw this.error(`${this.prop}: ${value} (${UNITS_UNKNOWN})`);
    throw this.error(
      `${this.prop}: ${value} (expected a length in pt, mm, cm, in, px, pc, rem, em, vw or vh, or a percentage)`,
    );
  }

  /**
   * A length (`12pt`, `5 mm`, `2EM`, `1e1pt`, `calc(10pt + 1em)`) in points, a percentage or
   * `auto`: one reader for `style` props and classes.
   */
  len(value: string): Len {
    const v = value.trim();
    if (v === 'auto') return 'auto';
    return this.dimToLen(this.evaluate(v, false), v);
  }

  lenPercent(value: string): LenPercent {
    const l = this.len(value);
    if (l === 'auto') throw this.error(`${this.prop}: auto is not allowed here`);
    return l;
  }

  points(value: string): number {
    const l = this.len(value);
    if (l === 'auto') throw this.error(`${this.prop}: auto is not allowed here`);
    if (typeof l !== 'number') throw this.error(`${this.prop}: ${value} (a percentage is not allowed here)`);
    return l;
  }

  /**
   * The value evaluated (`calc()` included), with its unit. `em` stays `em`, for a line height or
   * letter spacing relative to the text it applies to.
   */
  dim(value: string): Dim {
    return this.evaluate(value, true);
  }

  private evaluate(value: string, keepEm: boolean): Dim {
    try {
      return evalDim(value, this.units, keepEm);
    } catch (e) {
      if (!(e instanceof ValueError)) throw e;
      throw this.error(`${this.prop}: ${value} (${e.message})`);
    }
  }

  /**
   * A line height: a multiplier of the text's size (`1.5`, `150%`, `1.5em`), or a length in
   * points (`14pt`, `5mm`). `em` is the size of the text it applies to.
   */
  lineHeight(value: string): LineHeight {
    const d = this.dim(value);
    if (d.u === '' || d.u === 'em') return { mult: round(d.v) };
    if (d.u === '%') return { mult: round(d.v / 100) };
    if (d.u === 'pt') return { pt: round(d.v) };
    throw this.error(`${this.prop}: ${value}`);
  }

  /** A letter spacing: a length, `em` being the size of the text it applies to. */
  letterSpacing(value: string): LetterSpacing {
    const d = this.dim(value);
    if (d.u === 'em') return { em: round(d.v) };
    if (d.u === 'pt' || (d.u === '' && d.v === 0)) return { pt: round(d.v) };
    throw this.error(`${this.prop}: ${value}`);
  }

  num(value: string): number {
    const d = this.dim(value);
    if (d.u === '%') return round(d.v / 100);
    if (d.u !== '') throw this.error(`${this.prop}: ${value} (unexpected unit ${d.u})`);
    return round(d.v);
  }

  /** A `flex-grow` or `flex-shrink` factor: a number, not a percentage, that is not negative. */
  factor(value: string): number {
    const d = this.dim(value);
    if (d.u !== '') throw this.error(`${this.prop}: ${value} (expected a number)`);
    if (d.v < 0) throw this.error(`${this.prop}: ${value} (cannot be negative)`);
    return round(d.v);
  }

  color(value: string): ColorValue {
    try {
      return parseColor(value);
    } catch (e) {
      throw this.error(`${this.prop}: ${value} (${(e as Error).message})`);
    }
  }

  private breadth(v: string): TrackBreadth {
    if (/^\d+(\.\d+)?fr$/.test(v)) return v;
    if (v === 'auto' || v === 'min-content' || v === 'max-content') return v;
    const l = this.len(v);
    if (l === 'auto') return 'auto';
    return l;
  }

  trackSize(v: string): TrackSize {
    const mm = /^minmax\((.*)\)$/.exec(v);
    if (mm?.[1] !== undefined) {
      const [lo, hi] = splitTopLevel(mm[1]);
      if (lo === undefined || hi === undefined) throw this.error(`${this.prop}: bad minmax() in ${v}`);
      return { min: this.breadth(lo), max: this.breadth(hi) };
    }
    const fc = /^fit-content\((.*)\)$/.exec(v);
    if (fc?.[1] !== undefined) return { fitContent: this.lenPercent(fc[1]) };
    return this.breadth(v);
  }

  tracks(value: string): TemplateTrack[] {
    if (value.trim() === 'none') throw this.error(`${this.prop}: none`);
    return splitTopLevel(value, ' ').map((t): TemplateTrack => {
      const rep = /^repeat\((.*)\)$/.exec(t);
      if (rep?.[1] !== undefined) {
        const [count, ...rest] = splitTopLevel(rep[1]);
        const inner = splitTopLevel(rest.join(','), ' ').map((x) => this.trackSize(x));
        if (count === undefined || inner.length === 0) throw this.error(`${this.prop}: bad repeat()`);
        const n = count === 'auto-fill' || count === 'auto-fit' ? count : Number(count);
        if (typeof n === 'number' && !Number.isInteger(n)) throw this.error(`${this.prop}: bad repeat()`);
        return { repeat: n, tracks: inner as [TrackSize, ...TrackSize[]] };
      }
      return this.trackSize(t);
    });
  }

  /** `<start> [/ <end>]`, each a line, `span <n>` or `auto`. */
  placement(value: string): GridPlacement {
    const parts = value.split('/').map((s) => s.trim());
    if (parts.length > 2) throw this.error(`${this.prop}: ${value}`);
    const one = (s: string | undefined): GridPlacement => {
      if (s === undefined || s === 'auto') return {};
      const span = /^span\s+(\d+)$/.exec(s);
      if (span?.[1] !== undefined) {
        const n = Number(span[1]);
        if (n < 1 || n > MAX_GRID_SPAN) {
          throw this.error(`${this.prop}: ${value} (a span is 1 to ${MAX_GRID_SPAN} tracks)`);
        }
        return { span: n };
      }
      if (/^-?\d+$/.test(s)) {
        const n = Number(s);
        if (n === 0 || Math.abs(n) > MAX_GRID_LINE) {
          throw this.error(
            `${this.prop}: ${value} (a line is 1 to ${MAX_GRID_LINE}, or negative from the end; there is no line 0)`,
          );
        }
        return { start: n };
      }
      throw this.error(`${this.prop}: ${value}`);
    };
    const a = one(parts[0]);
    const b = one(parts[1]);
    const out: GridPlacement = { ...a };
    if (b.start !== undefined) out.end = b.start;
    // Of two spans, CSS ignores the one at the end.
    else if (b.span !== undefined) out.span ??= b.span;
    return out;
  }

  angle(value: string): number {
    const d = this.dim(value);
    if (d.u === 'deg' || (d.u === '' && d.v === 0)) return round(d.v);
    throw this.error(`${this.prop}: ${value} (expected an angle)`);
  }

  /** An integer from `min` to `max`. */
  integer(value: string, min: number, max = Number.POSITIVE_INFINITY): number {
    const n = this.num(value);
    if (!Number.isInteger(n) || n < min || n > max) {
      const range = max === Number.POSITIVE_INFINITY ? `≥ ${min}` : `from ${min} to ${max}`;
      throw this.error(`${this.prop}: ${value} (expected an integer ${range})`);
    }
    return n;
  }

  /**
   * CSS `text-decoration-line`. A run has one line, so both lines (`underline line-through`)
   * draw the line-through.
   */
  decorationLine(value: string): TextDecoration {
    const lines = new Set(value.trim().split(/\s+/));
    const known = ['none', 'underline', 'line-through'];
    if ([...lines].some((l) => !known.includes(l)) || (lines.has('none') && lines.size > 1)) {
      throw this.error(`${this.prop}: ${value} (supported: none, underline, line-through, or both lines)`);
    }
    return lines.has('line-through') ? 'lineThrough' : lines.has('underline') ? 'underline' : 'none';
  }

  radius(v: string): number {
    if (v.includes('/') || splitTopLevel(v, ' ').length > 1) {
      throw this.error(`${this.prop}: ${v} (elliptical corners are not supported)`);
    }
    return Math.min(this.points(v), INFINITE_PT);
  }
}

/**
 * Parses a CSS transform function list (`skewX(12deg) translate(2pt, 50%)`). Lengths take `em`,
 * `vw` and `vh` against `units`.
 */
export function parseTransform(prop: string, value: string, units?: Units): TransformOp[] {
  const r = new Reader(prop, units);
  const skew = (v: string): number => {
    const a = r.angle(v);
    if (a <= -90 || a >= 90) throw r.error(`${prop}: ${value} (a skew angle is between -90deg and 90deg)`);
    return a;
  };
  const out: TransformOp[] = [];
  for (const fn of splitTopLevel(value, ' ')) {
    const m = /^([a-zA-Z0-9]+)\((.*)\)$/.exec(fn);
    if (!m?.[1] || m[2] === undefined) throw r.error(`${prop}: ${value}`);
    const name = m[1];
    const args = splitTopLevel(m[2]);
    const [a = '0', b] = args;
    switch (name) {
      case 'translate':
        out.push({ translate: [r.lenPercent(a), r.lenPercent(b ?? '0')] });
        break;
      case 'translateX':
        out.push({ translate: [r.lenPercent(a), 0] });
        break;
      case 'translateY':
        out.push({ translate: [0, r.lenPercent(a)] });
        break;
      case 'scale':
        out.push({ scale: [r.num(a), r.num(b ?? a)] });
        break;
      case 'scaleX':
        out.push({ scale: [r.num(a), 1] });
        break;
      case 'scaleY':
        out.push({ scale: [1, r.num(a)] });
        break;
      case 'rotate':
      case 'rotateZ':
        out.push({ rotate: r.angle(a) });
        break;
      case 'skew':
        out.push({ skew: [skew(a), skew(b ?? '0')] });
        break;
      case 'skewX':
        out.push({ skew: [skew(a), 0] });
        break;
      case 'skewY':
        out.push({ skew: [0, skew(a)] });
        break;
      case 'matrix': {
        const n = args.map((x) => r.num(x));
        if (n.length !== 6) throw r.error(`${prop}: matrix() takes 6 numbers`);
        out.push({ matrix: n as [number, number, number, number, number, number] });
        break;
      }
      default:
        throw r.error(`${prop}: ${name}() (3D transforms and perspective are not supported)`);
    }
  }
  return out;
}

/**
 * Parses CSS `shape-outside` with a basic shape: `circle()`, `ellipse()`, `inset()` (corners
 * rounded by one length) or `polygon()`, in the margin box (`margin-box`, the default, may
 * follow). Lengths take `em`, `vw` and `vh` against `units`. `none` is the caller's to handle.
 */
export function parseShape(prop: string, value: string, units?: Units): ShapeOutside {
  const r = new Reader(prop, units);
  const parts = splitTopLevel(value.trim(), ' ');
  const box = parts[1];
  if (parts.length > 2 || (box !== undefined && box !== 'margin-box')) {
    throw r.error(
      `${prop}: ${value} (one basic shape, in the margin-box; images and other boxes are not supported)`,
    );
  }
  const m = /^([a-z]+)\((.*)\)$/.exec(parts[0] ?? '');
  if (!m?.[1] || m[2] === undefined)
    throw r.error(`${prop}: ${value} (expected circle(), ellipse(), inset() or polygon())`);
  const [, name, args] = m;
  // `<radii> at <position>`, for circles and ellipses.
  const words = splitTopLevel(args, ' ');
  const at = words.indexOf('at');
  const radii = at < 0 ? words : words.slice(0, at);
  const position = (): { at?: [LenPercent, LenPercent] } => {
    if (at < 0) return {};
    const p = words.slice(at + 1);
    if (p.length === 0 || p.length > 2) throw r.error(`${prop}: ${value} (a position is one or two values)`);
    return { at: parseOrigin(prop, p.join(' '), units) };
  };
  const radius = (v: string): LenPercent | undefined => {
    if (v === 'closest-side') return undefined;
    if (v === 'farthest-side') throw r.error(`${prop}: ${value} (farthest-side is not supported)`);
    const l = r.lenPercent(v);
    if (typeof l === 'number' ? l < 0 : l.startsWith('-'))
      throw r.error(`${prop}: ${value} (a radius cannot be negative)`);
    return l;
  };
  switch (name) {
    case 'circle': {
      if (radii.length > 1) throw r.error(`${prop}: ${value} (a circle has one radius)`);
      const rad = radii[0] === undefined ? undefined : radius(radii[0]);
      return { circle: { ...(rad === undefined ? {} : { radius: rad }), ...position() } };
    }
    case 'ellipse': {
      if (radii.length === 1 || radii.length > 2)
        throw r.error(`${prop}: ${value} (an ellipse has two radii, or none)`);
      const [rx, ry] = radii.map(radius);
      return {
        ellipse: { ...(rx === undefined ? {} : { rx }), ...(ry === undefined ? {} : { ry }), ...position() },
      };
    }
    case 'inset': {
      const round = words.indexOf('round');
      const sides = (round < 0 ? words : words.slice(0, round)).map((v) => r.lenPercent(v));
      const corners = round < 0 ? [] : words.slice(round + 1);
      if (sides.length === 0 || sides.length > 4)
        throw r.error(`${prop}: ${value} (inset() takes 1 to 4 offsets)`);
      if (round >= 0 && corners.length !== 1) {
        throw r.error(
          `${prop}: ${value} (one radius after round; per-corner and elliptical radii are not supported)`,
        );
      }
      const [top, right = top, bottom = top, left = right] = sides as [LenPercent, ...LenPercent[]];
      const rad = corners[0] === undefined ? undefined : r.radius(corners[0]);
      return { inset: { top, right, bottom, left, ...(rad === undefined ? {} : { radius: rad }) } };
    }
    case 'polygon': {
      const points = splitTopLevel(args);
      // The fill rule does not change the room a shape takes.
      if (points[0] === 'nonzero' || points[0] === 'evenodd') points.shift();
      if (points.length < 3) throw r.error(`${prop}: ${value} (a polygon has at least 3 corners)`);
      const corners = points.map((p): [LenPercent, LenPercent] => {
        const xy = splitTopLevel(p, ' ');
        if (xy.length !== 2) throw r.error(`${prop}: ${value} (a polygon's corners are x y pairs)`);
        return [r.lenPercent(xy[0] as string), r.lenPercent(xy[1] as string)];
      });
      return {
        polygon: corners as [[LenPercent, LenPercent], [LenPercent, LenPercent], [LenPercent, LenPercent]],
      };
    }
    default:
      throw r.error(`${prop}: ${name}() (supported: circle(), ellipse(), inset(), polygon())`);
  }
}

/** The position keywords of each axis, as the share of the box each stands for. */
const POSITION_X: ReadonlyMap<string, LenPercent> = new Map<string, LenPercent>([
  ['left', 0],
  ['center', '50%'],
  ['right', '100%'],
]);
const POSITION_Y: ReadonlyMap<string, LenPercent> = new Map<string, LenPercent>([
  ['top', 0],
  ['center', '50%'],
  ['bottom', '100%'],
]);
const isKeyword = (v: string) => POSITION_X.has(v) || POSITION_Y.has(v);
/** `left` or `right`: a keyword of the x axis only (`center` is both). */
const isOnlyX = (v: string) => v !== 'center' && POSITION_X.has(v);
/** `top` or `bottom`. */
const isOnlyY = (v: string) => v !== 'center' && POSITION_Y.has(v);

/** Two keywords that name y first (`top left`, `center right`), which CSS takes in either order. */
function keywordsYFirst(a: string, b: string): boolean {
  return isKeyword(a) && isKeyword(b) && (isOnlyY(a) || isOnlyX(b)) && !isOnlyY(b) && !isOnlyX(a);
}

/**
 * Parses `transform-origin` (keywords in either order, lengths, percentages). Lengths take `em`,
 * `vw` and `vh` against `units`.
 */
export function parseOrigin(prop: string, value: string, units?: Units): [LenPercent, LenPercent] {
  const r = new Reader(prop, units);
  const parts = splitTopLevel(value, ' ');
  if (parts.length === 0 || parts.length > 3) throw r.error(`${prop}: ${value}`);
  if (parts.length === 3 && parts[2] !== '0' && parts[2] !== '0px') {
    throw r.error(`${prop}: ${value} (no z offset)`);
  }
  let [a = 'center', b = 'center'] = parts;
  // Unlike `object-position`, a vertical keyword may also come before a length (`bottom 10%`).
  if (keywordsYFirst(a, b) || (isOnlyY(a) && !isKeyword(b))) [a, b] = [b, a];
  const x = POSITION_X.get(a) ?? r.lenPercent(a);
  const y = POSITION_Y.get(b) ?? r.lenPercent(b);
  return [x, y];
}

/**
 * Parses CSS `object-position`: one or two values, each a keyword, a length or a percentage. Two
 * keywords may come in either order (`top left`); otherwise the first value is x. A single value
 * leaves the other axis centred. Edge offsets (`right 10px bottom 5px`) are not supported.
 * `length` reads a value that is not a keyword (default: Tailwind's absolute units).
 */
export function parseObjectPosition(
  prop: string,
  value: string,
  length: (v: string) => LenPercent = (v) => new Reader(prop).lenPercent(v),
): ObjectPosition {
  const r = new Reader(prop);
  const parts = splitTopLevel(value, ' ');
  if (parts.length === 0 || parts.length > 2) {
    throw r.error(`${prop}: ${value} (one or two values; edge offsets are not supported)`);
  }
  let [a = 'center', b = 'center'] = parts;
  // Keywords alone may name y first (`top left`, or `top`, whose x is `center`).
  if (keywordsYFirst(a, b)) [a, b] = [b, a];
  if (isOnlyY(a) || isOnlyX(b)) {
    throw r.error(`${prop}: ${value} (expected x then y, e.g. "left top" or "25% 75%")`);
  }
  const x = POSITION_X.has(a) ? (a as 'left' | 'center' | 'right') : length(a);
  const y = POSITION_Y.has(b) ? (b as 'top' | 'center' | 'bottom') : length(b);
  return { x, y };
}
