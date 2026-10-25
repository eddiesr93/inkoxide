import {
  type FlatStyle,
  type FontStretch,
  parseFlex,
  parseFontFeatureSettings,
  parseFontStretch,
  parseFontVariantNumeric,
  parseFontVariationSettings,
  parseObjectPosition,
  parseOrigin,
  parseShape,
  parseTransform,
  round,
  splitTopLevel,
  type Units,
  UnsupportedValueError,
  ValueError,
} from '@inkoxide/style';
import {
  MAX_ASPECT_RATIO,
  MAX_FONT_SIZE,
  MAX_LINE_HEIGHT,
  MAX_LINES,
  MAX_WIDOW_LINES,
  MAX_Z_INDEX,
  MIN_Z_INDEX,
  parseFontFamily,
  Reader,
  substituteVars,
  UNITS_UNKNOWN,
  verticalAlignKeyword,
} from '@inkoxide/style/internal';

import type { CandidateCss, CandidateProblem, CandidateResult, Decl } from './candidate.ts';

/**
 * Where the classes come from: Tailwind's compiler (`TailwindCompiler`), or a table of the classes
 * a build saw (`precompiled.ts`).
 */
export interface ClassSource {
  /** The CSS of one class, or why it cannot be used. */
  candidate(cls: string): CandidateResult;
  /** Sorts classes the way Tailwind orders them in its stylesheet (later wins on conflicts). */
  sort(classes: string[]): string[];
  /** A theme variable (`--spacing`, `--color-red-500`), unresolved. */
  themeValue(name: string): string | undefined;
}

/**
 * `classes` in Tailwind's order, by their `rank` in its stylesheet (one without first), and as
 * given where ranks tie.
 */
export function sortByRank(classes: string[], rank: (cls: string) => bigint | null | undefined): string[] {
  if (classes.length < 2) return classes;
  return classes
    .map((cls, i) => ({ cls, rank: rank(cls) ?? -1n, i }))
    .sort((a, b) => (a.rank < b.rank ? -1 : a.rank > b.rank ? 1 : a.i - b.i))
    .map((x) => x.cls);
}

/** A class that cannot be turned into an engine style. Thrown in strict (dev) mode. */
export class UnsupportedUtilityError extends Error {
  override readonly name = 'UnsupportedUtilityError';
  /** The offending class, as written. */
  readonly className: string;
  /** Why: an unknown class, a variant, or the CSS property/value that has no engine equivalent. */
  readonly reason: string;
  /** The CSS property, when the class is known to Tailwind but its output is not supported. */
  readonly property?: string;
  constructor(className: string, reason: string, property?: string) {
    super(`Unsupported Tailwind class "${className}": ${reason}`);
    this.className = className;
    this.reason = reason;
    this.property = property;
  }
}

function problemReason(p: CandidateProblem): string {
  switch (p.kind) {
    case 'unknown':
      return 'not a Tailwind utility (check the spelling, or see docs/TAILWIND_SUPPORT.md)';
    case 'unseen':
      return (
        'not a Tailwind utility, or one the build did not see: with the Tailwind build plugin, ' +
        'write each class whole in the source (not put together at run time, as `bg-${c}-500` is), ' +
        "or list it in the plugin's `safelist`"
      );
    case 'media':
      return p.message;
    case 'atRule':
      return `at-rules have no meaning in a PDF (${p.rule})`;
    case 'screenSizes':
      return `it sizes the element by the screen width (${p.rule}); use max-w-* or w-full`;
    case 'selector':
      // `space-x-*` and `divide-*` style the element's children.
      if (/[>+~]/.test(p.selector)) {
        return `it styles other elements (${p.selector}); use gap instead of space-x/y and divide-*`;
      }
      return `it styles a part of the element that a PDF does not have (${p.selector})`;
  }
}

/** The variant prefixes of a class (`md:hover:` of `md:hover:p-4`), outside arbitrary values. */
export function variantOf(cls: string): string {
  let depth = 0;
  let end = 0;
  for (let i = 0; i < cls.length; i++) {
    const c = cls[i];
    if (c === '[' || c === '(') depth++;
    else if (c === ']' || c === ')') depth--;
    else if (c === ':' && depth === 0) end = i + 1;
  }
  return cls.slice(0, end);
}

/** The page variants, as errors and the docs name them. */
export const PAGE_VARIANTS: readonly string[] = [
  'sm: … 2xl:',
  'max-sm: … max-2xl:',
  'min-[…]:',
  'max-[…]:',
  'portrait:',
  'landscape:',
  'print:',
  '[@media(…)]:',
];

/**
 * Why class `cls` cannot be used, from Tailwind's `problem`: its own when the class without its
 * variants has it too (`md:space-x-2`), or else its variants'.
 */
function candidateReason(compiler: ClassSource, cls: string, problem: CandidateProblem): string {
  const variant = variantOf(cls);
  const unknown = (p: CandidateProblem) => p.kind === 'unknown' || p.kind === 'unseen';
  if (!variant || unknown(problem)) return problemReason(problem);
  const base = compiler.candidate(cls.slice(variant.length));
  if (!base.ok) return problemReason(unknown(base.problem) ? problem : base.problem);
  const why =
    problem.kind === 'media'
      ? ` (${problem.message})`
      : problem.kind === 'atRule'
        ? ` (${problem.rule})`
        : '';
  return `the variant \`${variant}\` has no meaning in a PDF${why}; page variants work: ${PAGE_VARIANTS.join(', ')}`;
}

/** `n`, read from `v`, when it is not negative: sizes, padding, gaps and border widths cannot be. */
function unsigned<T extends number | string>(r: Reader, v: string, n: T): T {
  if (typeof n === 'number' ? n < 0 : n.startsWith('-'))
    throw r.error(`${r.prop}: ${v} (cannot be negative)`);
  return n;
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
      s.flexBasis = unsigned(r, v, r.len(v));
      return;
    case 'gap': {
      const [row, column = row] = splitTopLevel(v, ' ');
      s.rowGap = unsigned(r, v, r.lenPercent(row ?? v));
      s.columnGap = unsigned(r, v, r.lenPercent(column ?? v));
      return;
    }
    case 'row-gap':
      s.rowGap = unsigned(r, v, r.lenPercent(v));
      return;
    case 'column-gap':
      s.columnGap = unsigned(r, v, r.lenPercent(v));
      return;
    case 'width':
    case 'height':
    case 'min-width':
    case 'min-height': {
      const key = prop.replace(/-(\w)/, (_, c: string) => c.toUpperCase());
      keyed[key] = unsigned(r, v, r.len(v));
      return;
    }
    case 'max-width':
    case 'max-height': {
      const key = prop === 'max-width' ? 'maxWidth' : 'maxHeight';
      if (v === 'none') delete keyed[key];
      else keyed[key] = unsigned(r, v, r.len(v));
      return;
    }
    case 'aspect-ratio': {
      const [a, b] = v.split('/').map((x) => x.trim());
      if (a === 'auto') {
        delete s.aspectRatio;
        return;
      }
      if (a === undefined) throw r.error(`aspect-ratio: ${v}`);
      const ratio = round(r.num(a) / (b === undefined ? 1 : r.num(b)));
      if (!(ratio > 0 && ratio <= MAX_ASPECT_RATIO)) {
        throw r.error(`aspect-ratio: ${v} (a ratio above 0 and at most ${MAX_ASPECT_RATIO})`);
      }
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
    case 'fill':
    case 'stroke': {
      const paint = v === 'none' ? 'none' : r.color(v);
      if (prop === 'fill') s.fill = paint;
      else s.stroke = paint;
      return;
    }
    case 'stroke-width': {
      // The drawing's units, which SVG writes as numbers or pixels.
      const n = r.num(v.replace(/px$/, ''));
      if (n < 0) throw r.error(`stroke-width: ${v} (not negative)`);
      s.strokeWidth = n;
      return;
    }
    case 'opacity': {
      const n = r.num(v);
      if (n < 0 || n > 1) throw r.error(`opacity: ${v} (between 0 and 1, or 0% and 100%)`);
      s.opacity = n;
      return;
    }
    case 'border-radius': {
      if (v.includes('/')) throw r.error(`border-radius: ${v} (elliptical corners are not supported)`);
      const [tl, tr, br, bl] = edges(v).map((x) => unsigned(r, x, r.radius(x))) as [
        number,
        number,
        number,
        number,
      ];
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
      else s.zIndex = r.integer(v, MIN_Z_INDEX, MAX_Z_INDEX);
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
      s.orphans = r.integer(v, 1, MAX_WIDOW_LINES);
      return;
    case 'widows':
      s.widows = r.integer(v, 1, MAX_WIDOW_LINES);
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
      s.borderSpacing = unsigned(r, v, r.points(x));
      return;
    }
    case 'vertical-align': {
      // `top`, `middle` and `bottom` for a cell; `baseline`, `super` and `sub` for text; and
      // `text-top` and `text-bottom` too for an image in text.
      const keyword = verticalAlignKeyword(v);
      if (keyword === undefined) throw r.error(`vertical-align: ${v}`);
      s.verticalAlign = keyword;
      return;
    }
    case 'font-feature-settings':
      s.fontFeatureSettings = parseFontFeatureSettings(prop, v);
      return;
    case 'font-variant-numeric':
      s.fontVariantNumeric = parseFontVariantNumeric(prop, v);
      return;
    case 'font-variation-settings':
      s.fontVariationSettings = parseFontVariationSettings(prop, v);
      return;
    case 'font-stretch':
      s.fontStretch = parseFontStretch(prop, v as FontStretch);
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
    case 'font-size': {
      const size = r.points(v);
      if (!(size > 0 && size <= MAX_FONT_SIZE)) {
        throw r.error(`font-size: ${v} (more than 0 and at most ${MAX_FONT_SIZE}pt)`);
      }
      s.fontSize = size;
      return;
    }
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
      const lh = r.lineHeight(v);
      if ('mult' in lh && !(lh.mult > 0 && lh.mult <= MAX_LINE_HEIGHT)) {
        throw r.error(`line-height: ${v} (a multiplier above 0 and at most ${MAX_LINE_HEIGHT})`);
      }
      s.lineHeight = lh;
      return;
    }
    case 'letter-spacing': {
      if (v === 'normal') {
        s.letterSpacing = { pt: 0 };
        return;
      }
      s.letterSpacing = r.letterSpacing(v);
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
    case 'float':
    case 'clear':
      // `float-start` and `clear-end`: boxes have no direction here.
      if (v.startsWith('inline-')) throw r.error(`${prop}: ${v} (use left or right)`);
      if (prop === 'float') s.float = r.oneOf('float', v);
      else s.clear = r.oneOf('clear', v);
      return;
    case 'shape-outside':
      if (v === 'none') delete s.shapeOutside;
      else s.shapeOutside = parseShape(prop, v, units);
      return;
    case 'shape-margin':
      s.shapeMargin = unsigned(r, v, r.points(v));
      return;
    case 'text-wrap':
      // `text-nowrap` is white space that does not wrap.
      if (v === 'nowrap') s.whiteSpace = 'nowrap';
      else s.textWrap = r.textWrap(v);
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
    keyed[`border${CORNERS[corner[1]]}Radius`] = unsigned(r, v, r.radius(v));
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
      if (family === 'padding') keyed[`padding${sd}`] = unsigned(r, raw, r.lenPercent(raw));
      else if (family === 'margin') keyed[`margin${sd}`] = r.len(raw);
      else if (family === 'inset') keyed[`inset${sd}`] = r.len(raw);
      else if (key === 'Width') keyed[`border${sd}Width`] = unsigned(r, raw, r.points(raw));
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
  compiler: ClassSource,
  classes: string[],
  onError: (e: UnsupportedUtilityError) => void,
  units?: TwUnits,
): { style: FlatStyle; relative: boolean } {
  const elementVars = new Map<string, string>();
  const initials = new Map<string, string>();
  const usable: string[] = [];
  // Read on use, to tell a style that depends on the units from one that can be cached alone.
  let relative = false;

  for (const cls of classes) {
    const r = compiler.candidate(cls);
    if (!r.ok) {
      onError(new UnsupportedUtilityError(cls, candidateReason(compiler, cls, r.problem)));
      continue;
    }
    if (r.css.decls.some(([, , media]) => media.length > 0)) {
      relative = true;
      if (!units) {
        onError(new UnsupportedUtilityError(cls, `its page variant ${UNITS_UNKNOWN}`));
        continue;
      }
    }
    usable.push(cls);
  }
  // Declarations for another page size are checked, not applied.
  const applies = ([, , media]: Decl): boolean =>
    media.every((q) => units !== undefined && q.matches(units.page));

  const ordered = compiler.sort(usable);
  for (const cls of ordered) {
    const r = compiler.candidate(cls);
    if (!r.ok) continue;
    for (const [name, value] of r.css.initials) initials.set(name, value);
    for (const d of r.css.decls) if (d[0].startsWith('--') && applies(d)) elementVars.set(d[0], d[1]);
  }
  const lookup = (name: string): string | undefined =>
    elementVars.get(name) ?? initials.get(name) ?? compiler.themeValue(name);

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
    applyClasses(compiler, ordered, applies, lookup, onError, (prop) =>
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

/**
 * Applies the declarations of `ordered` classes, each with the units `unitsFor` its property. Those
 * that do not `apply`, for another page size, are read and dropped: a mistake in them fails anyway.
 */
function applyClasses(
  compiler: ClassSource,
  ordered: string[],
  applies: (decl: Decl) => boolean,
  lookup: (name: string) => string | undefined,
  onError: (e: UnsupportedUtilityError) => void,
  unitsFor: (prop: string) => Units,
): FlatStyle {
  let applied: Applied = { style: {}, lines: {} };
  for (const cls of ordered) {
    const r = compiler.candidate(cls);
    if (!r.ok) continue;
    try {
      const next = applyDecls(r.css.decls.filter(applies), lookup, applied, unitsFor);
      const other = r.css.decls.filter((d) => !applies(d));
      // They read the class's own custom properties, which the element's leave out.
      if (other.length > 0) applyDecls(other, withOwnVars(r.css, lookup), applied, unitsFor);
      applied = next;
    } catch (e) {
      if (e instanceof UnsupportedValueError) onError(new UnsupportedUtilityError(cls, e.detail, e.property));
      else if (e instanceof ValueError) onError(new UnsupportedUtilityError(cls, e.message));
      else throw e;
    }
  }
  return applied.style;
}

/** A style as classes build it, and the grid lines it has read so far. */
interface Applied {
  style: FlatStyle;
  lines: GridLines;
}

/**
 * `decls` of one class applied over `to`, their custom properties read with `read`. Into a copy:
 * a failing class leaves no partial result behind.
 */
function applyDecls(
  decls: readonly Decl[],
  read: (name: string) => string | undefined,
  to: Applied,
  unitsFor: (prop: string) => Units,
): Applied {
  const next: FlatStyle = { ...to.style };
  const lines: GridLines = { ...to.lines };
  const clamp = decls.find(([p]) => p === '-webkit-line-clamp');
  // `truncate`: one line with an ellipsis. `text-ellipsis` alone has no engine meaning.
  const ellipsis =
    decls.some(([p, v]) => p === 'text-overflow' && v.trim() === 'ellipsis') &&
    decls.some(([p, v]) => p === 'white-space' && v.trim() === 'nowrap');
  if (clamp) {
    const n = clamp[1].trim();
    if (n === 'none' || n === 'unset') {
      delete next.maxLines;
      delete next.ellipsis;
    } else {
      next.maxLines = new Reader('-webkit-line-clamp').integer(n, 1, MAX_LINES);
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
      applyDecl(next, sideStyle ? 'border-style' : prop, substituteVars(raw, read), unitsFor(prop), lines);
    } catch (e) {
      // A value that does not parse at all (`var()` with no value, five edges): name the property.
      if (e instanceof ValueError) throw new UnsupportedValueError(prop, `${prop}: ${raw} (${e.message})`);
      throw e;
    }
  }
  return { style: next, lines };
}

/** `lookup`, after the custom properties `css` sets and the initial values it declares. */
function withOwnVars(
  css: CandidateCss,
  lookup: (name: string) => string | undefined,
): (name: string) => string | undefined {
  const own = new Map(css.initials);
  for (const [p, v] of css.decls) if (p.startsWith('--')) own.set(p, v);
  return (name) => own.get(name) ?? lookup(name);
}

/** Splits a class string into classes; the normalised string is the cache key. */
export function splitClasses(className: string): string[] {
  return className.split(/\s+/).filter((c) => c.length > 0);
}
