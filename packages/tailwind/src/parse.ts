import type { GridPlacement, Style, TemplateTrack, TrackBreadth, TrackSize } from '@inkoxide/schema';

import type { CandidateProblem, TailwindCompiler } from './compile.ts';
import type { ColorValue, Len, LenPercent, TransformOp, TwStyle } from './types.ts';
import {
  type Dim,
  evalDim,
  parseColor,
  parseFontFamily,
  round,
  splitTopLevel,
  substituteVars,
  ValueError,
} from './values.ts';

/** A class that cannot be turned into an engine style. Thrown in strict (dev) mode. */
export class UnsupportedUtilityError extends Error {
  override readonly name = 'UnsupportedUtilityError';
  constructor(
    /** The offending class, as written. */
    readonly className: string,
    /** Why: an unknown class, a variant, or the CSS property/value that has no engine equivalent. */
    readonly reason: string,
    /** The CSS property, when the class is known to Tailwind but its output is not supported. */
    readonly property?: string,
  ) {
    super(`Unsupported Tailwind class "${className}": ${reason}`);
  }
}

function problemReason(p: CandidateProblem): string {
  switch (p.kind) {
    case 'unknown':
      return 'not a Tailwind utility (check the spelling, or see docs/TAILWIND_SUPPORT.md)';
    case 'variant':
      return `variants and media queries have no meaning in a PDF (${p.detail})`;
    case 'selector':
      return `it styles other elements (${p.detail}); use gap instead of space-x/y and divide-*`;
  }
}

/** True when the class has a variant prefix (`md:`, `hover:`) outside an arbitrary value. */
function hasVariant(cls: string): boolean {
  let depth = 0;
  for (const c of cls) {
    if (c === '[' || c === '(') depth++;
    else if (c === ']' || c === ')') depth--;
    else if (c === ':' && depth === 0) return true;
  }
  return false;
}

const ENUMS = {
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
  borderStyle: ['solid', 'dashed', 'dotted'],
  gridAutoFlow: ['row', 'column', 'row dense', 'column dense'],
  whiteSpace: ['normal', 'nowrap', 'pre', 'pre-wrap', 'pre-line'],
  decorationStyle: ['solid', 'double', 'dotted', 'dashed', 'wavy'],
  direction: ['ltr', 'rtl'],
  textTransform: ['none', 'uppercase', 'lowercase', 'capitalize'],
} as const;

type EnumKey = keyof typeof ENUMS;

/** Reports a declaration the engine cannot express. */
class Unsupported extends Error {
  constructor(
    readonly property: string,
    readonly detail: string,
  ) {
    super(detail);
  }
}

function pick<K extends EnumKey>(key: K, prop: string, value: string): (typeof ENUMS)[K][number] {
  const v = value.trim() as (typeof ENUMS)[K][number];
  if (!(ENUMS[key] as readonly string[]).includes(v)) {
    throw new Unsupported(prop, `${prop}: ${value} (supported: ${ENUMS[key].join(', ')})`);
  }
  return v;
}

function dimToLen(d: Dim, prop: string, value: string): Len {
  if (d.u === 'pt') return round(d.v);
  if (d.u === '%') return `${round(d.v)}%`;
  if (d.u === '' && d.v === 0) return 0;
  if (d.u === '' && !Number.isFinite(d.v)) return 1e4;
  throw new Unsupported(prop, `${prop}: ${value} (expected a length in pt/px/rem or a percentage)`);
}

function len(prop: string, value: string): Len {
  const v = value.trim();
  if (v === 'auto') return 'auto';
  try {
    const d = evalDim(v);
    const out = dimToLen(d, prop, v);
    return typeof out === 'number' && out > 1e4 ? 1e4 : out;
  } catch (e) {
    if (e instanceof Unsupported) throw e;
    throw new Unsupported(prop, `${prop}: ${value} (${(e as Error).message})`);
  }
}

function lenPercent(prop: string, value: string): LenPercent {
  const l = len(prop, value);
  if (l === 'auto') throw new Unsupported(prop, `${prop}: auto is not allowed here`);
  return l;
}

function points(prop: string, value: string): number {
  const l = len(prop, value);
  if (typeof l !== 'number') throw new Unsupported(prop, `${prop}: ${value} (expected an absolute length)`);
  return l;
}

function num(prop: string, value: string): number {
  try {
    const d = evalDim(value);
    if (d.u === '%') return round(d.v / 100);
    if (d.u !== '') throw new ValueError(`unexpected unit ${d.u}`);
    return round(d.v);
  } catch (e) {
    throw new Unsupported(prop, `${prop}: ${value} (${(e as Error).message})`);
  }
}

function color(prop: string, value: string): ColorValue {
  try {
    return parseColor(value);
  } catch (e) {
    throw new Unsupported(prop, `${prop}: ${value} (${(e as Error).message})`);
  }
}

/** Expands a 1-4 value CSS edge shorthand into [top, right, bottom, left]. */
function edges(value: string): [string, string, string, string] {
  const p = splitTopLevel(value, ' ');
  const [a = '0', b = a, c = a, d = b] = p;
  if (p.length > 4) throw new ValueError(`too many values in "${value}"`);
  return [a, b, c, d];
}

function breadth(prop: string, v: string): TrackBreadth {
  if (/^\d+(\.\d+)?fr$/.test(v)) return v;
  if (v === 'auto' || v === 'min-content' || v === 'max-content') return v;
  const l = len(prop, v);
  if (l === 'auto') return 'auto';
  return l;
}

function trackSize(prop: string, v: string): TrackSize {
  const mm = /^minmax\((.*)\)$/.exec(v);
  if (mm?.[1] !== undefined) {
    const [lo, hi] = splitTopLevel(mm[1]);
    if (lo === undefined || hi === undefined) throw new Unsupported(prop, `${prop}: bad minmax() in ${v}`);
    return { min: breadth(prop, lo), max: breadth(prop, hi) };
  }
  const fc = /^fit-content\((.*)\)$/.exec(v);
  if (fc?.[1] !== undefined) return { fitContent: lenPercent(prop, fc[1]) };
  return breadth(prop, v);
}

function tracks(prop: string, value: string): TemplateTrack[] {
  if (value.trim() === 'none') throw new Unsupported(prop, `${prop}: none`);
  return splitTopLevel(value, ' ').map((t): TemplateTrack => {
    const rep = /^repeat\((.*)\)$/.exec(t);
    if (rep?.[1] !== undefined) {
      const [count, ...rest] = splitTopLevel(rep[1]);
      const inner = splitTopLevel(rest.join(','), ' ').map((x) => trackSize(prop, x));
      if (count === undefined || inner.length === 0) throw new Unsupported(prop, `${prop}: bad repeat()`);
      const n = count === 'auto-fill' || count === 'auto-fit' ? count : Number(count);
      if (typeof n === 'number' && !Number.isInteger(n)) throw new Unsupported(prop, `${prop}: bad repeat()`);
      return { repeat: n, tracks: inner as [TrackSize, ...TrackSize[]] };
    }
    return trackSize(prop, t);
  });
}

function placement(prop: string, value: string): GridPlacement {
  const parts = value.split('/').map((s) => s.trim());
  const one = (s: string | undefined): GridPlacement => {
    if (s === undefined || s === 'auto') return {};
    const span = /^span\s+(\d+)$/.exec(s);
    if (span?.[1] !== undefined) return { span: Number(span[1]) };
    if (/^-?\d+$/.test(s)) return { start: Number(s) };
    throw new Unsupported(prop, `${prop}: ${value}`);
  };
  const a = one(parts[0]);
  const b = one(parts[1]);
  const out: GridPlacement = { ...a };
  if (b.start !== undefined) out.end = b.start;
  return out;
}

function angle(prop: string, value: string): number {
  const d = evalDim(value);
  if (d.u === 'deg' || (d.u === '' && d.v === 0)) return round(d.v);
  throw new Unsupported(prop, `${prop}: ${value} (expected an angle)`);
}

function integer(prop: string, value: string, min: number): number {
  const n = num(prop, value);
  if (!Number.isInteger(n) || n < min)
    throw new Unsupported(prop, `${prop}: ${value} (expected an integer ≥ ${min})`);
  return n;
}

/** Parses a CSS transform function list (`skewX(12deg) translate(2pt, 50%)`). */
export function parseTransform(prop: string, value: string): TransformOp[] {
  const out: TransformOp[] = [];
  for (const fn of splitTopLevel(value, ' ')) {
    const m = /^([a-zA-Z0-9]+)\((.*)\)$/.exec(fn);
    if (!m?.[1] || m[2] === undefined) throw new Unsupported(prop, `${prop}: ${value}`);
    const name = m[1];
    const args = splitTopLevel(m[2]);
    const [a = '0', b] = args;
    switch (name) {
      case 'translate':
        out.push({ translate: [lenPercent(prop, a), lenPercent(prop, b ?? '0')] });
        break;
      case 'translateX':
        out.push({ translate: [lenPercent(prop, a), 0] });
        break;
      case 'translateY':
        out.push({ translate: [0, lenPercent(prop, a)] });
        break;
      case 'scale':
        out.push({ scale: [num(prop, a), num(prop, b ?? a)] });
        break;
      case 'scaleX':
        out.push({ scale: [num(prop, a), 1] });
        break;
      case 'scaleY':
        out.push({ scale: [1, num(prop, a)] });
        break;
      case 'rotate':
      case 'rotateZ':
        out.push({ rotate: angle(prop, a) });
        break;
      case 'skew':
        out.push({ skew: [angle(prop, a), angle(prop, b ?? '0')] });
        break;
      case 'skewX':
        out.push({ skew: [angle(prop, a), 0] });
        break;
      case 'skewY':
        out.push({ skew: [0, angle(prop, a)] });
        break;
      case 'matrix': {
        const n = args.map((x) => num(prop, x));
        if (n.length !== 6) throw new Unsupported(prop, `${prop}: matrix() takes 6 numbers`);
        out.push({ matrix: n as [number, number, number, number, number, number] });
        break;
      }
      default:
        throw new Unsupported(prop, `${prop}: ${name}() (3D transforms and perspective are not supported)`);
    }
  }
  return out;
}

const ORIGIN_X: Record<string, LenPercent> = { left: 0, center: '50%', right: '100%' };
const ORIGIN_Y: Record<string, LenPercent> = { top: 0, center: '50%', bottom: '100%' };

/** Parses `transform-origin` (keywords in either order, lengths, percentages). */
export function parseOrigin(prop: string, value: string): [LenPercent, LenPercent] {
  const parts = splitTopLevel(value, ' ');
  if (parts.length === 0 || parts.length > 3) throw new Unsupported(prop, `${prop}: ${value}`);
  if (parts.length === 3 && parts[2] !== '0' && parts[2] !== '0px') {
    throw new Unsupported(prop, `${prop}: ${value} (no z offset)`);
  }
  let [a = 'center', b = 'center'] = parts;
  // `top left` and `bottom 10%`: a vertical keyword first means the order is y x.
  if (a in ORIGIN_Y && a !== 'center' && (parts.length === 1 || !(b in ORIGIN_Y) || b === 'center')) {
    if (parts.length === 1) b = 'center';
    [a, b] = [b, a];
  }
  const x = ORIGIN_X[a] ?? lenPercent(prop, a);
  const y = ORIGIN_Y[b] ?? lenPercent(prop, b);
  return [x, y];
}

const CORNERS: Record<string, 'TopLeft' | 'TopRight' | 'BottomRight' | 'BottomLeft'> = {
  'top-left': 'TopLeft',
  'top-right': 'TopRight',
  'bottom-right': 'BottomRight',
  'bottom-left': 'BottomLeft',
  // Logical corners, left-to-right.
  'start-start': 'TopLeft',
  'start-end': 'TopRight',
  'end-end': 'BottomRight',
  'end-start': 'BottomLeft',
};

function radius(prop: string, v: string): number {
  if (v.includes('/') || splitTopLevel(v, ' ').length > 1) {
    throw new Unsupported(prop, `${prop}: ${v} (elliptical corners are not supported)`);
  }
  return Math.min(points(prop, v), 1e4);
}

const SIDES = ['Top', 'Right', 'Bottom', 'Left'] as const;
type Side = (typeof SIDES)[number];

function sidesOf(logical: string): readonly Side[] {
  switch (logical) {
    case '':
      return SIDES;
    case 'top':
      return ['Top'];
    case 'right':
    case 'inline-end':
      return ['Right'];
    case 'bottom':
      return ['Bottom'];
    case 'left':
    case 'inline-start':
      return ['Left'];
    case 'inline':
      return ['Left', 'Right'];
    case 'block':
      return ['Top', 'Bottom'];
    default:
      throw new ValueError(`unknown side ${logical}`);
  }
}

type W = Record<string, unknown>;

/** Applies one resolved declaration to the style being built. */
function applyDecl(s: TwStyle, prop: string, value: string): void {
  const w = s as W;
  const v = value.trim();
  switch (prop) {
    case 'display':
      s.display = pick('display', prop, v);
      return;
    case 'flex-direction':
      s.flexDirection = pick('flexDirection', prop, v);
      return;
    case 'flex-wrap':
      s.flexWrap = pick('flexWrap', prop, v);
      return;
    case 'justify-content':
      s.justifyContent = pick('justifyContent', prop, v);
      return;
    case 'align-items':
      s.alignItems = pick('alignItems', prop, v);
      return;
    case 'align-self':
      s.alignSelf = pick('alignSelf', prop, v);
      return;
    case 'align-content':
      s.alignContent = pick('alignContent', prop, v);
      return;
    case 'justify-items':
      s.justifyItems = pick('justifyItems', prop, v);
      return;
    case 'justify-self':
      s.justifySelf = pick('justifySelf', prop, v);
      return;
    case 'flex': {
      const words = splitTopLevel(v, ' ');
      if (v === 'none') {
        s.flexGrow = 0;
        s.flexShrink = 0;
        s.flexBasis = 'auto';
      } else if (v === 'auto') {
        s.flexGrow = 1;
        s.flexShrink = 1;
        s.flexBasis = 'auto';
      } else if (words.length === 1 && /^[\d.]+$/.test(v)) {
        s.flexGrow = Number(v);
        s.flexShrink = 1;
        s.flexBasis = '0%';
      } else {
        const [g, second, third] = words;
        if (g === undefined || second === undefined) throw new Unsupported(prop, `flex: ${v}`);
        s.flexGrow = num(prop, g);
        if (third === undefined && !/^[\d.]+$/.test(second)) {
          // `flex: <grow> <basis>`
          s.flexShrink = 1;
          s.flexBasis = len(prop, second);
        } else {
          s.flexShrink = num(prop, second);
          s.flexBasis = third === undefined ? '0%' : len(prop, third);
        }
      }
      return;
    }
    case 'flex-grow':
      s.flexGrow = num(prop, v);
      return;
    case 'flex-shrink':
      s.flexShrink = num(prop, v);
      return;
    case 'flex-basis':
      s.flexBasis = len(prop, v);
      return;
    case 'gap': {
      const [r, c = r] = splitTopLevel(v, ' ');
      s.rowGap = lenPercent(prop, r ?? v);
      s.columnGap = lenPercent(prop, c ?? v);
      return;
    }
    case 'row-gap':
      s.rowGap = lenPercent(prop, v);
      return;
    case 'column-gap':
      s.columnGap = lenPercent(prop, v);
      return;
    case 'width':
    case 'height':
    case 'min-width':
    case 'min-height': {
      const key = prop.replace(/-(\w)/, (_, c: string) => c.toUpperCase());
      w[key] = len(prop, v);
      return;
    }
    case 'max-width':
    case 'max-height': {
      const key = prop === 'max-width' ? 'maxWidth' : 'maxHeight';
      if (v === 'none') delete w[key];
      else w[key] = len(prop, v);
      return;
    }
    case 'aspect-ratio': {
      const [a, b] = v.split('/').map((x) => x.trim());
      if (a === undefined || a === 'auto') throw new Unsupported(prop, `aspect-ratio: ${v}`);
      s.aspectRatio = round(num(prop, a) / (b === undefined ? 1 : num(prop, b)));
      return;
    }
    case 'position':
      if (v.trim() === 'fixed') {
        throw new Unsupported(
          prop,
          `position: fixed (supported: relative, absolute; to repeat an element on every page, give it the fixed prop and position: absolute)`,
        );
      }
      s.position = pick('position', prop, v);
      return;
    case 'overflow':
      s.overflow = pick('overflow', prop, v === 'clip' ? 'hidden' : v);
      return;
    case 'grid-template-columns':
      s.gridTemplateColumns = tracks(prop, v);
      return;
    case 'grid-template-rows':
      s.gridTemplateRows = tracks(prop, v);
      return;
    case 'grid-auto-columns':
      s.gridAutoColumns = splitTopLevel(v, ' ').map((t) => trackSize(prop, t));
      return;
    case 'grid-auto-rows':
      s.gridAutoRows = splitTopLevel(v, ' ').map((t) => trackSize(prop, t));
      return;
    case 'grid-auto-flow':
      s.gridAutoFlow = pick('gridAutoFlow', prop, v);
      return;
    case 'grid-column':
      s.gridColumn = placement(prop, v);
      return;
    case 'grid-row':
      s.gridRow = placement(prop, v);
      return;
    case 'grid-column-start':
    case 'grid-column-end':
    case 'grid-row-start':
    case 'grid-row-end': {
      const key = prop.startsWith('grid-column') ? 'gridColumn' : 'gridRow';
      const p: GridPlacement = { ...s[key] };
      const one = placement(prop, v);
      if (prop.endsWith('start')) {
        delete p.start;
        Object.assign(p, one);
      } else {
        delete p.end;
        if (one.start !== undefined) p.end = one.start;
        else if (one.span !== undefined) p.span = one.span;
      }
      s[key] = p;
      return;
    }
    case 'background-color':
      s.backgroundColor = color(prop, v);
      return;
    case 'opacity':
      s.opacity = num(prop, v);
      return;
    case 'border-radius': {
      if (v.includes('/'))
        throw new Unsupported(prop, `border-radius: ${v} (elliptical corners are not supported)`);
      const [tl, tr, br, bl] = edges(v).map((x) => radius(prop, x)) as [number, number, number, number];
      for (const c of Object.values(CORNERS)) delete w[`border${c}Radius`];
      if (tl === tr && tr === br && br === bl) s.borderRadius = tl;
      else {
        delete s.borderRadius;
        Object.assign(s, {
          borderTopLeftRadius: tl,
          borderTopRightRadius: tr,
          borderBottomRightRadius: br,
          borderBottomLeftRadius: bl,
        });
      }
      return;
    }
    case 'border-style': {
      if (v === 'none') {
        for (const sd of SIDES) w[`border${sd}Width`] = 0;
        return;
      }
      for (const sd of SIDES) delete w[`border${sd}Style`];
      s.borderStyle = pick('borderStyle', prop, v);
      return;
    }
    case 'z-index':
      if (v === 'auto') delete s.zIndex;
      else s.zIndex = integer(prop, v, Number.MIN_SAFE_INTEGER);
      return;
    case 'translate': {
      if (v === 'none') {
        delete s.translate;
        return;
      }
      const [x = '0', y = '0', z] = splitTopLevel(v, ' ');
      if (z !== undefined && evalDim(z).v !== 0) throw new Unsupported(prop, `translate: ${v} (no z axis)`);
      s.translate = [lenPercent(prop, x), lenPercent(prop, y)];
      return;
    }
    case 'scale': {
      if (v === 'none') {
        delete s.scale;
        return;
      }
      const [x = '1', y = x] = splitTopLevel(v, ' ');
      s.scale = [num(prop, x), num(prop, y)];
      return;
    }
    case 'transform':
      if (v === 'none' || v === '') delete s.transform;
      else s.transform = parseTransform(prop, v);
      return;
    case 'transform-origin':
      s.transformOrigin = parseOrigin(prop, v);
      return;
    case 'text-decoration-color': {
      const c = color(prop, v);
      if (c === 'currentColor') delete s.decorationColor;
      else s.decorationColor = c;
      return;
    }
    case 'text-decoration-thickness':
      if (v === 'auto' || v === 'from-font') delete s.decorationThickness;
      else s.decorationThickness = points(prop, v);
      return;
    case 'text-decoration-style':
      s.decorationStyle = pick('decorationStyle', prop, v);
      return;
    case 'text-indent':
      s.textIndent = points(prop, v);
      return;
    case 'direction':
      s.direction = pick('direction', prop, v);
      return;
    case 'orphans':
      s.orphans = integer(prop, v, 1);
      return;
    case 'widows':
      s.widows = integer(prop, v, 1);
      return;
    case 'border-collapse':
      if (v !== 'collapse' && v !== 'separate') throw new Unsupported(prop, `border-collapse: ${v}`);
      s.borderCollapse = v;
      return;
    case 'border-spacing': {
      const [x, y = x] = splitTopLevel(v, ' ');
      if (x === undefined || (y !== undefined && y !== x && points(prop, y) !== points(prop, x))) {
        throw new Unsupported(prop, `border-spacing: ${v} (one value for both axes)`);
      }
      s.borderSpacing = points(prop, x);
      return;
    }
    case 'vertical-align':
      if (v !== 'top' && v !== 'middle' && v !== 'bottom')
        throw new Unsupported(prop, `vertical-align: ${v}`);
      s.verticalAlign = v;
      return;
    case 'background-image':
      throw new Unsupported(prop, 'gradients and background images are not supported (use bg-<colour>)');
    case 'rotate': {
      const d = evalDim(v);
      if (d.u !== 'deg') throw new Unsupported(prop, `rotate: ${v}`);
      s.rotate = round(d.v);
      return;
    }
    case 'font-family':
      s.fontFamily = parseFontFamily(v);
      return;
    case 'font-weight':
      s.fontWeight = num(prop, v);
      return;
    case 'font-style':
      if (v === 'normal') s.fontStyle = 'normal';
      else if (v === 'italic' || v.startsWith('oblique')) s.fontStyle = 'italic';
      else throw new Unsupported(prop, `font-style: ${v}`);
      return;
    case 'font-size':
      s.fontSize = points(prop, v);
      return;
    case 'color': {
      const c = color(prop, v);
      if (c === 'currentColor') throw new Unsupported(prop, 'color: currentColor');
      s.color = c;
      return;
    }
    case 'line-height': {
      if (v === 'normal') {
        delete s.lineHeight;
        return;
      }
      const d = evalDim(v);
      if (d.u === '') s.lineHeight = { mult: round(d.v) };
      else if (d.u === '%') s.lineHeight = { mult: round(d.v / 100) };
      else if (d.u === 'em') s.lineHeight = { mult: round(d.v) };
      else if (d.u === 'pt') s.lineHeight = { pt: round(d.v) };
      else throw new Unsupported(prop, `line-height: ${v}`);
      return;
    }
    case 'letter-spacing': {
      if (v === 'normal') {
        s.letterSpacing = { pt: 0 };
        return;
      }
      const d = evalDim(v);
      if (d.u === 'em') s.letterSpacing = { em: round(d.v) };
      else if (d.u === 'pt' || (d.u === '' && d.v === 0)) s.letterSpacing = { pt: round(d.v) };
      else throw new Unsupported(prop, `letter-spacing: ${v}`);
      return;
    }
    case 'text-align':
      if (v === 'left' || v === 'center' || v === 'right' || v === 'justify' || v === 'start' || v === 'end')
        s.textAlign = v;
      else throw new Unsupported(prop, `text-align: ${v}`);
      return;
    case 'text-decoration-line':
      if (v === 'underline') s.textDecoration = 'underline';
      else if (v === 'line-through') s.textDecoration = 'lineThrough';
      else if (v === 'none') s.textDecoration = 'none';
      else throw new Unsupported(prop, `text-decoration-line: ${v}`);
      return;
    case 'text-transform':
      s.textTransform = pick('textTransform', prop, v);
      return;
    case 'white-space':
      s.whiteSpace = pick('whiteSpace', prop, v);
      return;
    case 'hyphens':
      s.hyphenate = v === 'auto';
      return;
    case '-webkit-hyphens':
      return;
    case 'top':
    case 'right':
    case 'bottom':
    case 'left':
      w[`inset${prop[0]?.toUpperCase()}${prop.slice(1)}`] = len(prop, v);
      return;
  }

  const corner = /^border-([a-z]+-[a-z]+)-radius$/.exec(prop);
  if (corner?.[1] && CORNERS[corner[1]]) {
    w[`border${CORNERS[corner[1]]}Radius`] = radius(prop, v);
    return;
  }

  // Edge families: padding, margin, inset, border width/colour/style.
  const m =
    /^(padding|margin|inset|border)(?:-(top|right|bottom|left|inline|block|inline-start|inline-end))?(?:-(width|color|style))?$/.exec(
      prop,
    );
  if (m) {
    const [, family, side = '', part] = m;
    if (family === 'border' && part === 'style') {
      const style = pick('borderStyle', prop, v);
      if (side === '') {
        for (const sd of SIDES) delete w[`border${sd}Style`];
        s.borderStyle = style;
      } else {
        for (const sd of sidesOf(side)) w[`border${sd}Style`] = style;
      }
      return;
    }
    if (family === 'border' && part === undefined) {
      throw new Unsupported(prop, `${prop}: ${v} (use border width and colour utilities)`);
    }
    if (family !== 'border' && part !== undefined) throw new Unsupported(prop, `${prop}: ${v}`);
    const values = side === '' ? edges(v) : null;
    const key = family === 'border' ? (part === 'width' ? 'Width' : 'Color') : '';
    for (const sd of sidesOf(side)) {
      const raw = values ? (values[SIDES.indexOf(sd)] ?? v) : v;
      if (family === 'padding') w[`padding${sd}`] = lenPercent(prop, raw);
      else if (family === 'margin') w[`margin${sd}`] = len(prop, raw);
      else if (family === 'inset') w[`inset${sd}`] = len(prop, raw);
      else if (key === 'Width') w[`border${sd}Width`] = points(prop, raw);
      else w[`border${sd}Color`] = color(prop, raw);
    }
    return;
  }

  throw new Unsupported(prop, `the CSS property "${prop}" has no Inkoxide equivalent`);
}

/**
 * Maps a list of classes to one style, resolving CSS custom properties the way a browser would
 * for a single element: `--tw-*` values set by any class on the element, then `@property`
 * initial values, then the theme. Conflicts are settled by Tailwind's own stylesheet order.
 */
export function mapClasses(
  compiler: TailwindCompiler,
  classes: string[],
  onError: (e: UnsupportedUtilityError) => void,
): TwStyle {
  let style: TwStyle = {};
  const elementVars = new Map<string, string>();
  const initials = new Map<string, string>();
  const usable: string[] = [];

  for (const cls of classes) {
    if (hasVariant(cls)) {
      onError(new UnsupportedUtilityError(cls, 'variants (md:, hover:, print:, …) have no meaning in a PDF'));
      continue;
    }
    const r = compiler.candidate(cls);
    if (!r.ok) {
      onError(new UnsupportedUtilityError(cls, problemReason(r.problem)));
      continue;
    }
    usable.push(cls);
  }

  const ordered = compiler.sort(usable);
  for (const cls of ordered) {
    const r = compiler.candidate(cls);
    if (!r.ok) continue;
    for (const [name, value] of r.css.initials) initials.set(name, value);
    for (const [prop, value] of r.css.decls) if (prop.startsWith('--')) elementVars.set(prop, value);
  }
  const lookup = (name: string): string | undefined =>
    elementVars.get(name) ?? initials.get(name) ?? compiler.themeValue(name);

  for (const cls of ordered) {
    const r = compiler.candidate(cls);
    if (!r.ok) continue;
    const decls = r.css.decls;
    const clamp = decls.find(([p]) => p === '-webkit-line-clamp');
    // `truncate`: one line with an ellipsis. `text-ellipsis` alone has no engine meaning.
    const ellipsis =
      decls.some(([p, v]) => p === 'text-overflow' && v.trim() === 'ellipsis') &&
      decls.some(([p, v]) => p === 'white-space' && v.trim() === 'nowrap');
    // Apply into a scratch copy so a failing class leaves no partial result behind.
    const next: TwStyle = { ...style };
    try {
      if (clamp) {
        const n = clamp[1].trim();
        if (n === 'none' || n === 'unset') {
          delete next.maxLines;
          delete next.ellipsis;
        } else {
          next.maxLines = num('-webkit-line-clamp', n);
          next.ellipsis = true;
        }
      } else if (ellipsis) {
        next.maxLines = 1;
        next.ellipsis = true;
      }
      for (const [prop, raw] of decls) {
        if (prop.startsWith('--')) continue;
        if (clamp && (prop === 'overflow' || prop === 'display' || prop.startsWith('-webkit-'))) continue;
        if (ellipsis && (prop === 'overflow' || prop === 'text-overflow' || prop === 'white-space')) continue;
        if (prop === 'text-overflow') throw new Unsupported(prop, `text-overflow: ${raw} (use truncate)`);
        // `border-b` sets `border-bottom-style: var(--tw-border-style)`: the box's one style.
        const sideStyle = /^border-[a-z-]+-style$/.test(prop) && raw.trim() === 'var(--tw-border-style)';
        applyDecl(next, sideStyle ? 'border-style' : prop, substituteVars(raw, lookup));
      }
      style = next;
    } catch (e) {
      if (e instanceof Unsupported) onError(new UnsupportedUtilityError(cls, e.detail, e.property));
      else if (e instanceof ValueError) onError(new UnsupportedUtilityError(cls, e.message));
      else throw e;
    }
  }
  return style;
}

/** Splits a class string into classes; the normalised string is the cache key. */
export function splitClasses(className: string): string[] {
  return className.split(/\s+/).filter((c) => c.length > 0);
}

/** Exposed for the schema-type tests. */
export type { Style };
