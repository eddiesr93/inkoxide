import {
  type FlatStyle,
  parseFlex,
  parseFontFeatureSettings,
  parseFontVariantNumeric,
  parseObjectPosition,
  parseOrigin,
  parseTransform,
  round,
  splitTopLevel,
  type Units,
  UnsupportedValueError,
  ValueError,
} from '@inkoxide/style';
import { parseFontFamily, Reader, substituteVars, UNITS_UNKNOWN } from '@inkoxide/style/internal';

import type { CandidateProblem, TailwindCompiler } from './compile.ts';

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
      if (p.detail.startsWith('@media')) return `media queries have no meaning in a PDF (${p.detail})`;
      if (p.detail.startsWith('@')) return `at-rules have no meaning in a PDF (${p.detail})`;
      // `space-x-*` and `divide-*` style the element's children.
      if (/[>+~]/.test(p.detail)) {
        return `it styles other elements (${p.detail}); use gap instead of space-x/y and divide-*`;
      }
      return `it styles a part of the element that a PDF does not have (${p.detail})`;
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

/** Expands a 1-4 value CSS edge shorthand into [top, right, bottom, left]. */
function edges(value: string): [string, string, string, string] {
  const p = splitTopLevel(value, ' ');
  const [a = '0', b = a, c = a, d = b] = p;
  if (p.length > 4) throw new ValueError(`too many values in "${value}"`);
  return [a, b, c, d];
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

/** The two halves of each grid placement as written, `auto` where unset. */
type GridLines = Partial<Record<'gridColumn' | 'gridRow', readonly [start: string, end: string]>>;

/**
 * Applies one resolved declaration to the style being built, with its lengths in `units`. `lines`
 * holds the grid placements as written, for a longhand to replace one half of.
 */
function applyDecl(s: FlatStyle, prop: string, value: string, units: Units, lines: GridLines): void {
  const r = new Reader(prop, units);
  // The style by key, for the keys built from the property name (`paddingTop`, `insetLeft`).
  const keyed = s as Record<string, unknown>;
  const v = value.trim();
  switch (prop) {
    case 'display':
      s.display = r.oneOf('display', v);
      return;
    case 'flex-direction':
      s.flexDirection = r.oneOf('flexDirection', v);
      return;
    case 'flex-wrap':
      s.flexWrap = r.oneOf('flexWrap', v);
      return;
    case 'justify-content':
      s.justifyContent = r.oneOf('justifyContent', v);
      return;
    case 'align-items':
      s.alignItems = r.oneOf('alignItems', v);
      return;
    case 'align-self':
      s.alignSelf = r.oneOf('alignSelf', v);
      return;
    case 'align-content':
      s.alignContent = r.oneOf('alignContent', v);
      return;
    case 'justify-items':
      s.justifyItems = r.oneOf('justifyItems', v);
      return;
    case 'justify-self':
      s.justifySelf = r.oneOf('justifySelf', v);
      return;
    case 'flex':
      [s.flexGrow, s.flexShrink, s.flexBasis] = parseFlex(
        v,
        (x) => r.num(x),
        (x) => r.len(x),
      );
      return;
    case 'flex-grow':
      s.flexGrow = r.factor(v);
      return;
    case 'flex-shrink':
      s.flexShrink = r.factor(v);
      return;
    case 'flex-basis':
      s.flexBasis = r.len(v);
      return;
    case 'gap': {
      const [row, column = row] = splitTopLevel(v, ' ');
      s.rowGap = r.lenPercent(row ?? v);
      s.columnGap = r.lenPercent(column ?? v);
      return;
    }
    case 'row-gap':
      s.rowGap = r.lenPercent(v);
      return;
    case 'column-gap':
      s.columnGap = r.lenPercent(v);
      return;
    case 'width':
    case 'height':
    case 'min-width':
    case 'min-height': {
      const key = prop.replace(/-(\w)/, (_, c: string) => c.toUpperCase());
      keyed[key] = r.len(v);
      return;
    }
    case 'max-width':
    case 'max-height': {
      const key = prop === 'max-width' ? 'maxWidth' : 'maxHeight';
      if (v === 'none') delete keyed[key];
      else keyed[key] = r.len(v);
      return;
    }
    case 'aspect-ratio': {
      const [a, b] = v.split('/').map((x) => x.trim());
      if (a === undefined || a === 'auto') throw r.error(`aspect-ratio: ${v}`);
      const ratio = round(r.num(a) / (b === undefined ? 1 : r.num(b)));
      if (!(ratio > 0 && ratio <= 1e6)) throw r.error(`aspect-ratio: ${v} (a ratio above 0)`);
      s.aspectRatio = ratio;
      return;
    }
    case 'position':
      if (v.trim() === 'fixed') {
        throw r.error(
          `position: fixed (supported: relative, absolute; to repeat an element on every page, give it the fixed prop and position: absolute)`,
        );
      }
      s.position = r.oneOf('position', v);
      return;
    case 'overflow':
      s.overflow = r.oneOf('overflow', v === 'clip' ? 'hidden' : v);
      return;
    case 'grid-template-columns':
      s.gridTemplateColumns = r.tracks(v);
      return;
    case 'grid-template-rows':
      s.gridTemplateRows = r.tracks(v);
      return;
    case 'grid-auto-columns':
      s.gridAutoColumns = splitTopLevel(v, ' ').map((t) => r.trackSize(t));
      return;
    case 'grid-auto-rows':
      s.gridAutoRows = splitTopLevel(v, ' ').map((t) => r.trackSize(t));
      return;
    case 'grid-auto-flow':
      // `dense` alone is `row dense`.
      s.gridAutoFlow = r.oneOf('gridAutoFlow', v === 'dense' ? 'row dense' : v);
      return;
    case 'grid-column':
    case 'grid-row': {
      const key = prop === 'grid-column' ? 'gridColumn' : 'gridRow';
      s[key] = r.placement(v);
      const [start = 'auto', end = 'auto'] = v.split('/').map((x) => x.trim());
      lines[key] = [start, end];
      return;
    }
    case 'grid-column-start':
    case 'grid-column-end':
    case 'grid-row-start':
    case 'grid-row-end': {
      const key = prop.startsWith('grid-column') ? 'gridColumn' : 'gridRow';
      // One half alone, read first so that an error names it.
      if (v.includes('/')) throw r.error(`${prop}: ${v}`);
      r.placement(v);
      const [start, end] = lines[key] ?? ['auto', 'auto'];
      const both = prop.endsWith('start') ? ([v, end] as const) : ([start, v] as const);
      lines[key] = both;
      // Read as `<start> / <end>`, as CSS does: two lines replace a span set before them
      // (`col-span-3 col-start-1 col-end-3` is 1 / 3), and a span at the end is ignored after one
      // at the start.
      s[key] = r.placement(`${both[0]} / ${both[1]}`);
      return;
    }
    case 'background-color':
      s.backgroundColor = r.color(v);
      return;
    case 'opacity': {
      const n = r.num(v);
      if (n < 0 || n > 1) throw r.error(`opacity: ${v} (between 0 and 1, or 0% and 100%)`);
      s.opacity = n;
      return;
    }
    case 'border-radius': {
      if (v.includes('/')) throw r.error(`border-radius: ${v} (elliptical corners are not supported)`);
      const [tl, tr, br, bl] = edges(v).map((x) => r.radius(x)) as [number, number, number, number];
      for (const c of Object.values(CORNERS)) delete keyed[`border${c}Radius`];
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
        for (const sd of SIDES) keyed[`border${sd}Width`] = 0;
        return;
      }
      for (const sd of SIDES) delete keyed[`border${sd}Style`];
      s.borderStyle = r.oneOf('borderStyle', v);
      return;
    }
    case 'z-index':
      if (v === 'auto') delete s.zIndex;
      else s.zIndex = r.integer(v, Number.MIN_SAFE_INTEGER);
      return;
    case 'translate': {
      if (v === 'none') {
        delete s.translate;
        return;
      }
      const [x = '0', y = '0', z] = splitTopLevel(v, ' ');
      if (z !== undefined && r.dim(z).v !== 0) throw r.error(`translate: ${v} (no z axis)`);
      s.translate = [r.lenPercent(x), r.lenPercent(y)];
      return;
    }
    case 'scale': {
      if (v === 'none') {
        delete s.scale;
        return;
      }
      const [x = '1', y = x] = splitTopLevel(v, ' ');
      s.scale = [r.num(x), r.num(y)];
      return;
    }
    case 'transform':
      if (v === 'none' || v === '') delete s.transform;
      else s.transform = parseTransform(prop, v, units);
      return;
    case 'transform-origin':
      s.transformOrigin = parseOrigin(prop, v, units);
      return;
    case 'object-fit':
      s.objectFit = r.oneOf('objectFit', v);
      return;
    case 'object-position':
      s.objectPosition = parseObjectPosition(prop, v, (x) => r.lenPercent(x));
      return;
    case 'text-decoration-color': {
      const c = r.color(v);
      if (c === 'currentColor') delete s.decorationColor;
      else s.decorationColor = c;
      return;
    }
    case 'text-decoration-thickness':
      if (v === 'auto' || v === 'from-font') delete s.decorationThickness;
      else s.decorationThickness = r.points(v);
      return;
    case 'text-decoration-style':
      s.decorationStyle = r.oneOf('decorationStyle', v);
      return;
    case 'text-indent':
      s.textIndent = r.points(v);
      return;
    case 'direction':
      s.direction = r.oneOf('direction', v);
      return;
    case 'orphans':
      s.orphans = r.integer(v, 1);
      return;
    case 'widows':
      s.widows = r.integer(v, 1);
      return;
    case 'border-collapse':
      if (v !== 'collapse' && v !== 'separate') throw r.error(`border-collapse: ${v}`);
      s.borderCollapse = v;
      return;
    case 'border-spacing': {
      const [x, y = x] = splitTopLevel(v, ' ');
      if (x === undefined || (y !== undefined && y !== x && r.points(y) !== r.points(x))) {
        throw r.error(`border-spacing: ${v} (one value for both axes)`);
      }
      s.borderSpacing = r.points(x);
      return;
    }
    case 'vertical-align':
      // `top`, `middle` and `bottom` for a cell; `baseline`, `super` and `sub` for text.
      if (!['top', 'middle', 'bottom', 'baseline', 'super', 'sub'].includes(v))
        throw r.error(`vertical-align: ${v}`);
      s.verticalAlign = v as NonNullable<FlatStyle['verticalAlign']>;
      return;
    case 'font-feature-settings':
      s.fontFeatureSettings = parseFontFeatureSettings(prop, v);
      return;
    case 'font-variant-numeric':
      s.fontVariantNumeric = parseFontVariantNumeric(prop, v);
      return;
    case 'background-image':
      throw r.error('gradients and background images are not supported (use bg-<colour>)');
    case 'rotate': {
      if (v === 'none') {
        delete s.rotate;
        return;
      }
      const d = r.dim(v);
      if (d.u !== 'deg') throw r.error(`rotate: ${v}`);
      s.rotate = round(d.v);
      return;
    }
    case 'font-family':
      s.fontFamily = parseFontFamily(v);
      return;
    case 'font-weight':
      s.fontWeight = r.num(v);
      return;
    case 'font-style':
      if (v === 'normal') s.fontStyle = 'normal';
      else if (v === 'italic' || v.startsWith('oblique')) s.fontStyle = 'italic';
      else throw r.error(`font-style: ${v}`);
      return;
    case 'font-size':
      s.fontSize = r.points(v);
      return;
    case 'color': {
      const c = r.color(v);
      if (c === 'currentColor') throw r.error('color: currentColor');
      s.color = c;
      return;
    }
    case 'line-height': {
      if (v === 'normal') {
        delete s.lineHeight;
        return;
      }
      const d = r.dim(v);
      if (d.u === '') s.lineHeight = { mult: round(d.v) };
      else if (d.u === '%') s.lineHeight = { mult: round(d.v / 100) };
      else if (d.u === 'em') s.lineHeight = { mult: round(d.v) };
      else if (d.u === 'pt') s.lineHeight = { pt: round(d.v) };
      else throw r.error(`line-height: ${v}`);
      return;
    }
    case 'letter-spacing': {
      if (v === 'normal') {
        s.letterSpacing = { pt: 0 };
        return;
      }
      const d = r.dim(v);
      if (d.u === 'em') s.letterSpacing = { em: round(d.v) };
      else if (d.u === 'pt' || (d.u === '' && d.v === 0)) s.letterSpacing = { pt: round(d.v) };
      else throw r.error(`letter-spacing: ${v}`);
      return;
    }
    case 'text-align':
      if (v === 'left' || v === 'center' || v === 'right' || v === 'justify' || v === 'start' || v === 'end')
        s.textAlign = v;
      else throw r.error(`text-align: ${v}`);
      return;
    case 'text-decoration-line':
      s.textDecoration = r.decorationLine(v);
      return;
    case 'text-transform':
      s.textTransform = r.oneOf('textTransform', v);
      return;
    case 'white-space':
      s.whiteSpace = r.oneOf('whiteSpace', v);
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
      keyed[`inset${prop[0]?.toUpperCase()}${prop.slice(1)}`] = r.len(v);
      return;
  }

  const corner = /^border-([a-z]+-[a-z]+)-radius$/.exec(prop);
  if (corner?.[1] && CORNERS[corner[1]]) {
    keyed[`border${CORNERS[corner[1]]}Radius`] = r.radius(v);
    return;
  }

  // Edge families: padding, margin, inset, border width/colour/style.
  const m =
    /^(padding|margin|inset|border)(?:-(top|right|bottom|left|inline|block|inline-start|inline-end))?(?:-(width|color|style))?$/.exec(
      prop,
    );
  if (m) {
    const [, family, side = '', part] = m;
    // Per side: `border-style` itself is read in the switch above.
    if (family === 'border' && part === 'style') {
      const style = r.oneOf('borderStyle', v);
      for (const sd of sidesOf(side)) keyed[`border${sd}Style`] = style;
      return;
    }
    if (family === 'border' && part === undefined) {
      throw r.error(`${prop}: ${v} (use border width and colour utilities)`);
    }
    if (family !== 'border' && part !== undefined) throw r.error(`${prop}: ${v}`);
    const values = side === '' ? edges(v) : null;
    const key = family === 'border' ? (part === 'width' ? 'Width' : 'Color') : '';
    for (const sd of sidesOf(side)) {
      const raw = values ? (values[SIDES.indexOf(sd)] ?? v) : v;
      if (family === 'padding') keyed[`padding${sd}`] = r.lenPercent(raw);
      else if (family === 'margin') keyed[`margin${sd}`] = r.len(raw);
      else if (family === 'inset') keyed[`inset${sd}`] = r.len(raw);
      else if (key === 'Width') keyed[`border${sd}Width`] = r.points(raw);
      else keyed[`border${sd}Color`] = r.color(raw);
    }
    return;
  }

  throw r.error(`the CSS property "${prop}" has no Inkoxide equivalent`);
}

/** What `em`, `vw` and `vh` in a class string's values resolve against, in points. */
export interface TwUnits {
  /** The inherited font size: `em` in `text-[…]`, and in other values unless the element sets one. */
  parentFontSize: number;
  /** The element's font size when its `style` sets it, which wins over the classes' own. */
  fontSize?: number | undefined;
  /** The page size, for `vw` and `vh` (1% of the width and height). */
  page: { width: number; height: number };
}

/**
 * Maps a list of classes to one style, resolving CSS custom properties the way a browser would
 * for a single element: `--tw-*` values set by any class on the element, then `@property`
 * initial values, then the theme. Conflicts are settled by Tailwind's own stylesheet order.
 * `relative` tells whether the style depends on `units` (a value in `em`, `vw` or `vh`); without
 * `units`, such a value is an error.
 */
export function mapClasses(
  compiler: TailwindCompiler,
  classes: string[],
  onError: (e: UnsupportedUtilityError) => void,
  units?: TwUnits,
): { style: FlatStyle; relative: boolean } {
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

  // Read on use, to tell a style that depends on the units from one that can be cached alone.
  let relative = false;
  let ownEm = false;
  const unitsOf = (em: () => number, own: boolean): Units => ({
    get em() {
      relative = true;
      ownEm ||= own;
      if (!units) throw new ValueError(UNITS_UNKNOWN);
      return em();
    },
    get page() {
      relative = true;
      if (!units) throw new ValueError(UNITS_UNKNOWN);
      return units.page;
    },
  });
  const parentEm = () => units?.parentFontSize ?? 0;
  const fontUnits = unitsOf(parentEm, false);
  const apply = (em: () => number): FlatStyle =>
    applyClasses(compiler, ordered, lookup, onError, (prop) =>
      prop === 'font-size' ? fontUnits : unitsOf(em, true),
    );

  let style = apply(() => units?.fontSize ?? parentEm());
  // `em` is the element's font size, which a class may set after the lengths that use it.
  const classSize = style.fontSize;
  if (units && ownEm && units.fontSize === undefined && classSize !== undefined && classSize !== parentEm()) {
    style = apply(() => classSize);
  }
  return { style, relative };
}

/** Applies the declarations of `ordered` classes, each with the units `unitsFor` its property. */
function applyClasses(
  compiler: TailwindCompiler,
  ordered: string[],
  lookup: (name: string) => string | undefined,
  onError: (e: UnsupportedUtilityError) => void,
  unitsFor: (prop: string) => Units,
): FlatStyle {
  let style: FlatStyle = {};
  let lines: GridLines = {};
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
    const next: FlatStyle = { ...style };
    const nextLines: GridLines = { ...lines };
    try {
      if (clamp) {
        const n = clamp[1].trim();
        if (n === 'none' || n === 'unset') {
          delete next.maxLines;
          delete next.ellipsis;
        } else {
          next.maxLines = new Reader('-webkit-line-clamp').integer(n, 1);
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
        if (prop === 'text-overflow')
          throw new UnsupportedValueError(prop, `text-overflow: ${raw} (use truncate)`);
        // `border-b` sets `border-bottom-style: var(--tw-border-style)`: the box's one style.
        const sideStyle = /^border-[a-z-]+-style$/.test(prop) && raw.trim() === 'var(--tw-border-style)';
        try {
          applyDecl(
            next,
            sideStyle ? 'border-style' : prop,
            substituteVars(raw, lookup),
            unitsFor(prop),
            nextLines,
          );
        } catch (e) {
          // A value that does not parse at all (`var()` with no value, five edges): name the property.
          if (e instanceof ValueError)
            throw new UnsupportedValueError(prop, `${prop}: ${raw} (${e.message})`);
          throw e;
        }
      }
      style = next;
      lines = nextLines;
    } catch (e) {
      if (e instanceof UnsupportedValueError) onError(new UnsupportedUtilityError(cls, e.detail, e.property));
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
