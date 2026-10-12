import type { Color, Style, TransformOp } from '@inkoxide/schema';
import {
  type BorderStyle,
  type ColorValue,
  type LenPercent,
  type ObjectFit,
  type ObjectPosition,
  parseColor,
  parseObjectPosition,
  parseOrigin,
  parseTransform,
  PT_PER_PX,
  PT_PER_REM,
  TEXT_INHERITED,
  TEXT_ONLY,
  type TextDecoration,
  type TwStyle,
} from '@inkoxide/tailwind';

import type { HyphenationCallback, StyleProp, Style as StyleInput } from '../components/index.ts';

/** A text property that cascades from an element to its content. */
type InheritedKey = (typeof TEXT_INHERITED)[number];

/** The keys of `TEXT_INHERITED` that are set at the top of every document. */
type RootKey = 'fontFamily' | 'fontWeight' | 'fontStyle' | 'fontSize' | 'color';

/**
 * The text properties in effect at a point of the tree, after inheritance: every
 * `TEXT_INHERITED` key, unset until an element sets one, apart from the font and the colour,
 * which the document sets.
 */
export interface TextCtx extends Omit<{ [K in InheritedKey]?: TwStyle[K] }, RootKey> {
  fontFamily: readonly string[];
  fontWeight: number;
  fontStyle: 'normal' | 'italic';
  fontSize: number;
  color: Color;
  lang: string | undefined;
  /**
   * Splits words into syllables in place of the built-in patterns (`hyphenationCallback`), unless
   * `hyphenate` is `false`: read it with `callbackOf`.
   */
  hyphenation: HyphenationCallback | undefined;
  /** Link target inherited by runs (nested `Text href`). */
  link: string | undefined;
  /** Resolved font id, cached by the builder (reset when the font properties change). */
  fontId?: string | undefined;
}

const inheritedFlag = new WeakMap<TwStyle, boolean>();

/** True when the style sets any inherited text property. Cached per (immutable) style object. */
export function hasInherited(s: TwStyle): boolean {
  let v = inheritedFlag.get(s);
  if (v === undefined) {
    v = TEXT_INHERITED.some((k) => s[k] !== undefined);
    inheritedFlag.set(s, v);
  }
  return v;
}

export interface InheritExtra {
  lang?: string | undefined;
  hyphenate?: boolean | undefined;
  /** `null` turns off the callback inherited from the document. */
  hyphenation?: HyphenationCallback | null | undefined;
  link?: string | undefined;
  direction?: TwStyle['direction'];
  orphans?: number | undefined;
  widows?: number | undefined;
}

/** The context of an element's content: the parent's, overridden by the element's own style. */
export function inherit(parent: TextCtx, own: TwStyle, extra?: InheritExtra): TextCtx {
  const changes = hasInherited(own);
  if (!changes && (!extra || Object.values(extra).every((v) => v === undefined))) {
    return parent;
  }
  const c: TextCtx = { ...parent };
  if (changes) {
    // Another face: the builder resolves the font again.
    if (own.fontFamily !== undefined || own.fontWeight !== undefined || own.fontStyle !== undefined) {
      c.fontId = undefined;
    }
    const set = c as Record<InheritedKey, unknown>;
    for (const k of TEXT_INHERITED) if (own[k] !== undefined) set[k] = own[k];
  }
  if (extra) {
    if (extra.lang !== undefined) c.lang = extra.lang;
    if (extra.hyphenate !== undefined) c.hyphenate = extra.hyphenate;
    if (extra.hyphenation !== undefined) {
      c.hyphenation = extra.hyphenation ?? undefined;
      // A callback given to this element applies even where an ancestor turned hyphenation off.
      const ownSwitch = own.hyphenate ?? extra.hyphenate;
      if (extra.hyphenation && c.hyphenate === false && ownSwitch === undefined) c.hyphenate = undefined;
    }
    if (extra.link !== undefined) c.link = extra.link;
    if (extra.direction !== undefined) c.direction = extra.direction;
    if (extra.orphans !== undefined) c.orphans = extra.orphans;
    if (extra.widows !== undefined) c.widows = extra.widows;
  }
  return c;
}

/** The hyphenation callback in effect: none where `hyphenate` (or `hyphens-none`) turns it off. */
export function callbackOf(ctx: TextCtx): HyphenationCallback | undefined {
  return ctx.hyphenate === false ? undefined : ctx.hyphenation;
}

/** Whether the engine hyphenates with its patterns: `hyphenate` is on and no callback replaces them. */
export function usesPatterns(ctx: TextCtx): boolean {
  return ctx.hyphenate === true && ctx.hyphenation === undefined;
}

const resolveColor = (c: ColorValue, current: Color): Color => (c === 'currentColor' ? current : c);

const SIMPLE_KEYS = [
  'display',
  'flexDirection',
  'flexWrap',
  'justifyContent',
  'alignItems',
  'alignSelf',
  'alignContent',
  'justifyItems',
  'justifySelf',
  'flexGrow',
  'flexShrink',
  'flexBasis',
  'width',
  'height',
  'minWidth',
  'minHeight',
  'maxWidth',
  'maxHeight',
  'aspectRatio',
  'position',
  'gridTemplateColumns',
  'gridTemplateRows',
  'gridAutoColumns',
  'gridAutoRows',
  'gridAutoFlow',
  'gridColumn',
  'gridRow',
  'overflow',
  'opacity',
  'transformOrigin',
  'zIndex',
] as const satisfies ReadonlyArray<keyof TwStyle & keyof Style>;

const CORNER_KEYS = [
  ['topLeft', 'borderTopLeftRadius'],
  ['topRight', 'borderTopRightRadius'],
  ['bottomRight', 'borderBottomRightRadius'],
  ['bottomLeft', 'borderBottomLeftRadius'],
] as const;

/** Keys that belong to the box of an element (everything except text properties). */
export const BOX_KEYS: ReadonlySet<string> = new Set<string>([
  ...SIMPLE_KEYS,
  'rowGap',
  'columnGap',
  'backgroundColor',
  'borderStyle',
  'borderCollapse',
  'borderSpacing',
  'verticalAlign',
  'borderRadius',
  'rotate',
  'translate',
  'scale',
  'transform',
  ...CORNER_KEYS.map(([, k]) => k),
  ...['Top', 'Right', 'Bottom', 'Left'].map((s) => `border${s}Style`),
  ...['padding', 'margin', 'inset'].flatMap((p) => ['Top', 'Right', 'Bottom', 'Left'].map((s) => `${p}${s}`)),
  ...['Top', 'Right', 'Bottom', 'Left'].flatMap((s) => [`border${s}Width`, `border${s}Color`]),
]);

const TEXT_ONLY_KEYS: ReadonlySet<string> = new Set(TEXT_ONLY);

/** Box keys set by a style, for "this utility has no effect here" errors. */
export function boxKeys(s: TwStyle): string[] {
  return Object.keys(s).filter((k) => BOX_KEYS.has(k) && s[k as keyof TwStyle] !== undefined);
}

export function textOnlyKeys(s: TwStyle): string[] {
  return Object.keys(s).filter((k) => TEXT_ONLY_KEYS.has(k) && s[k as keyof TwStyle] !== undefined);
}

type Edge = 'top' | 'right' | 'bottom' | 'left';
const EDGES: ReadonlyArray<[Edge, 'Top' | 'Right' | 'Bottom' | 'Left']> = [
  ['top', 'Top'],
  ['right', 'Right'],
  ['bottom', 'Bottom'],
  ['left', 'Left'],
];

type EdgeKeys = readonly [Edge, keyof TwStyle][];
const keysFor = (prefix: string, suffix = ''): EdgeKeys =>
  EDGES.map(([edge, cap]) => [edge, `${prefix}${cap}${suffix}` as keyof TwStyle] as [Edge, keyof TwStyle]);
const PADDING = keysFor('padding');
const MARGIN = keysFor('margin');
const INSET = keysFor('inset');
const BORDER_WIDTH = keysFor('border', 'Width');
const BORDER_COLOR = keysFor('border', 'Color');
const BORDER_STYLE = keysFor('border', 'Style');

function edgeObject<T>(s: TwStyle, keys: EdgeKeys): Partial<Record<Edge, T>> | undefined {
  let out: Partial<Record<Edge, T>> | undefined;
  for (const [edge, key] of keys) {
    const v = s[key];
    if (v !== undefined) {
      out ??= {};
      out[edge] = v as T;
    }
  }
  return out;
}

interface BoxPlan {
  /** The style when it does not depend on the text colour. */
  fixed: Style | undefined;
  /** True when a border or background resolves to `currentColor`. */
  usesCurrent: boolean;
  byColor?: Map<Color, Style | undefined>;
}

const plans = new WeakMap<TwStyle, BoxPlan>();
const cellPlans = new WeakMap<TwStyle, BoxPlan>();

function usesCurrentColor(s: TwStyle): boolean {
  if (s.backgroundColor === 'currentColor') return true;
  for (let i = 0; i < 4; i++) {
    const w = s[BORDER_WIDTH[i]?.[1] as keyof TwStyle] as number | undefined;
    const c = s[BORDER_COLOR[i]?.[1] as keyof TwStyle];
    if (c === 'currentColor' || (c === undefined && w !== undefined && w > 0)) return true;
  }
  return false;
}

/**
 * The schema `Style` for an element's box. `currentColor` borders and backgrounds take the
 * element's text colour; a border side with a width but no colour does too, as in CSS.
 * Returns undefined when nothing is set. Results are cached per (immutable) style object, so
 * callers must copy before modifying the returned object.
 */
export function boxStyle(s: TwStyle, currentColor: Color, cell = false): Style | undefined {
  const cache = cell ? cellPlans : plans;
  let plan = cache.get(s);
  if (!plan) {
    const usesCurrent = usesCurrentColor(s);
    plan = { fixed: usesCurrent ? undefined : computeBox(s, currentColor, cell), usesCurrent };
    cache.set(s, plan);
  }
  if (!plan.usesCurrent) return plan.fixed;
  plan.byColor ??= new Map();
  if (plan.byColor.has(currentColor)) return plan.byColor.get(currentColor);
  const out = computeBox(s, currentColor, cell);
  plan.byColor.set(currentColor, out);
  return out;
}

function computeBox(s: TwStyle, currentColor: Color, cell: boolean): Style | undefined {
  const out: Style = {};
  const w = out as Record<string, unknown>;
  let any = false;
  for (const k of SIMPLE_KEYS) {
    const v = s[k];
    if (v !== undefined) {
      w[k] = v;
      any = true;
    }
  }
  if (s.rowGap !== undefined && s.rowGap === s.columnGap) {
    out.gap = s.rowGap;
    any = true;
  } else {
    if (s.rowGap !== undefined) {
      out.rowGap = s.rowGap;
      any = true;
    }
    if (s.columnGap !== undefined) {
      out.columnGap = s.columnGap;
      any = true;
    }
  }
  const padding = edgeObject<number | `${number}%`>(s, PADDING);
  if (padding) {
    out.padding = padding;
    any = true;
  }
  const margin = edgeObject<number | `${number}%` | 'auto'>(s, MARGIN);
  if (margin) {
    out.margin = margin;
    any = true;
  }
  const inset = edgeObject<number | `${number}%` | 'auto'>(s, INSET);
  if (inset) {
    out.inset = inset;
    any = true;
  }
  if (s.backgroundColor !== undefined) {
    out.backgroundColor = resolveColor(s.backgroundColor, currentColor);
    any = true;
  }
  const widths = edgeObject<number>(s, BORDER_WIDTH);
  const colors: Partial<Record<Edge, Color>> = {};
  let anyColor = false;
  for (let i = 0; i < 4; i++) {
    const [edge, ck] = BORDER_COLOR[i] as [Edge, keyof TwStyle];
    const c = s[ck] as ColorValue | undefined;
    const wv = widths?.[edge];
    if (c !== undefined) colors[edge] = resolveColor(c, currentColor);
    else if (wv !== undefined && wv > 0) colors[edge] = currentColor;
    else continue;
    anyColor = true;
  }
  if (widths) {
    out.borderWidth = widths;
    any = true;
  }
  if (anyColor) {
    out.borderColor = colors;
    any = true;
  }
  const sideStyles = edgeObject<NonNullable<TwStyle['borderStyle']>>(s, BORDER_STYLE);
  if (sideStyles) {
    const base = s.borderStyle ?? 'solid';
    out.borderStyle = {
      top: sideStyles.top ?? base,
      right: sideStyles.right ?? base,
      bottom: sideStyles.bottom ?? base,
      left: sideStyles.left ?? base,
    };
    any = true;
  } else if (s.borderStyle !== undefined && s.borderStyle !== 'solid') {
    out.borderStyle = s.borderStyle;
    any = true;
  }
  if (CORNER_KEYS.some(([, k]) => s[k] !== undefined)) {
    const base = s.borderRadius ?? 0;
    const r: Record<string, number> = {};
    for (const [corner, k] of CORNER_KEYS) r[corner] = s[k] ?? base;
    out.borderRadius = r;
    any = true;
  } else if (s.borderRadius !== undefined) {
    out.borderRadius = s.borderRadius;
    any = true;
  }
  // CSS order: translate, rotate, scale, then the transform list. A lone rotation keeps the
  // engine's `rotate` (smaller trees, same result).
  if (s.translate !== undefined || s.scale !== undefined || (s.transform?.length ?? 0) > 0) {
    const ops: TransformOp[] = [];
    if (s.translate) ops.push({ translate: s.translate });
    if (s.rotate !== undefined) ops.push({ rotate: s.rotate });
    if (s.scale) ops.push({ scale: s.scale });
    if (s.transform) ops.push(...s.transform);
    out.transform = ops;
    any = true;
  } else if (s.rotate !== undefined) {
    out.rotate = s.rotate;
    any = true;
  }
  if (s.borderCollapse !== undefined) {
    out.borderCollapse = s.borderCollapse;
    any = true;
  }
  if (s.borderSpacing !== undefined) {
    out.borderSpacing = s.borderSpacing;
    any = true;
  }
  if (cell && s.verticalAlign !== undefined) {
    out.verticalAlign = s.verticalAlign;
    any = true;
  }
  return any ? out : undefined;
}

const round = (n: number): number => Math.round(n * 1e6) / 1e6;

/** Adds the text-node properties (alignment, line height, clamping, hyphenation) to a style. */
export function textNodeStyle(
  base: Style | undefined,
  own: TwStyle,
  ctx: TextCtx,
  docLang: string | undefined,
) {
  let out = base;
  const set = <K extends keyof Style>(k: K, v: Style[K]): void => {
    out = out === base ? { ...base } : out;
    (out as Style)[k] = v;
  };
  // `start` is the engine default; `left` is the same in a left-to-right paragraph.
  const ltr = ctx.direction === undefined || ctx.direction === 'ltr';
  if (ctx.textAlign !== undefined && ctx.textAlign !== 'start' && !(ctx.textAlign === 'left' && ltr)) {
    set('textAlign', ctx.textAlign);
  }
  if (ctx.direction !== undefined && ctx.direction !== 'ltr') set('direction', ctx.direction);
  if (ctx.textIndent !== undefined && ctx.textIndent !== 0) set('textIndent', ctx.textIndent);
  if (ctx.orphans !== undefined && ctx.orphans !== 1) set('orphans', ctx.orphans);
  if (ctx.widows !== undefined && ctx.widows !== 1) set('widows', ctx.widows);
  if (ctx.lineHeight !== undefined) {
    const lh = 'mult' in ctx.lineHeight ? ctx.lineHeight.mult : ctx.lineHeight.pt / ctx.fontSize;
    set('lineHeight', round(lh));
  }
  if (own.maxLines !== undefined) set('maxLines', own.maxLines);
  if (own.ellipsis) set('ellipsis', true);
  if (ctx.whiteSpace !== undefined && ctx.whiteSpace !== 'normal') set('whiteSpace', ctx.whiteSpace);
  // A hyphenation callback replaces the patterns: the engine would break the words it keeps whole.
  if (usesPatterns(ctx)) set('hyphenate', true);
  if (ctx.lang !== undefined && ctx.lang !== docLang) set('lang', ctx.lang);
  return out;
}

// ---------------------------------------------------------------------------------------------
// The `style` prop

type Edges<T> = T | { top?: T; right?: T; bottom?: T; left?: T };

function toColor(v: string | Color): ColorValue {
  return typeof v === 'string' ? parseColor(v) : v;
}

function spread<T>(
  out: Record<string, unknown>,
  prefix: string,
  suffix: string,
  v: Edges<T> | undefined,
  map = (x: T): unknown => x,
) {
  if (v === undefined) return;
  if (typeof v === 'object' && v !== null && !Array.isArray(v)) {
    for (const [edge, cap] of EDGES) {
      const x = (v as Record<Edge, T | undefined>)[edge];
      if (x !== undefined) out[`${prefix}${cap}${suffix}`] = map(x);
    }
  } else {
    for (const [, cap] of EDGES) out[`${prefix}${cap}${suffix}`] = map(v as T);
  }
}

/** Points per unit. `rem` is the root font size Tailwind uses. */
const PT_PER: Record<string, number> = {
  pt: 1,
  mm: 72 / 25.4,
  cm: 720 / 25.4,
  in: 72,
  px: PT_PER_PX,
  pc: 12,
  rem: PT_PER_REM,
};

/** What relative units resolve against, in points. */
export interface Units {
  /** The font size, for `em`. */
  em: number;
  /** The page size, for `vw` and `vh` (1% of the width and height). */
  page: { width: number; height: number };
}

/**
 * Points from a length: a number, or a string with a unit (`'20mm'`). Percentages and `auto`
 * pass through. `em`, `vw` and `vh` need `units`.
 */
export function toPt<T>(v: number | string | T, units?: Units): number | T | string {
  if (typeof v !== 'string') return v;
  if (v === 'auto' || /^-?(\d+\.?\d*|\.\d+)%$/.test(v)) return v;
  const m = /^(-?(?:\d+\.?\d*|\.\d+))\s*(pt|mm|cm|in|px|pc|rem|em|vw|vh)$/.exec(v.trim());
  if (!m?.[1] || !m[2])
    throw new Error(
      `"${v}" is not a length (use a number of points or a unit: pt, mm, cm, in, px, pc, rem, em, vw, vh)`,
    );
  const unit = m[2];
  let per = PT_PER[unit];
  if (per === undefined) {
    if (!units) throw new Error(`"${v}" depends on the font or page size, which is not known here`);
    per = unit === 'em' ? units.em : (unit === 'vw' ? units.page.width : units.page.height) / 100;
  }
  const pt = round(Number(m[1]) * per);
  if (!Number.isFinite(pt)) throw new Error(`"${v}" is too large a length`);
  return pt;
}

const LENGTH_KEYS = [
  'fontSize',
  'width',
  'height',
  'minWidth',
  'minHeight',
  'maxWidth',
  'maxHeight',
  'flexBasis',
  'rowGap',
  'columnGap',
  'borderSpacing',
  'textIndent',
  'decorationThickness',
] as const;

function sides(
  out: Record<string, unknown>,
  prop: 'padding' | 'margin' | 'inset',
  axis: { x?: string | number; y?: string | number },
  each: Record<'Top' | 'Right' | 'Bottom' | 'Left', string | number | undefined>,
  units: Units | undefined,
): void {
  const set = (side: string, v: string | number | undefined) => {
    if (v !== undefined) out[`${prop}${side}`] = v === 'auto' ? 'auto' : toPt(v, units);
  };
  set('Left', axis.x);
  set('Right', axis.x);
  set('Top', axis.y);
  set('Bottom', axis.y);
  for (const [side, v] of Object.entries(each)) set(side, v);
}

/**
 * Applies an element's `style` prop to its className style. `em` is the element's font size
 * (the inherited `parentSize` in `fontSize` itself), and `vw` and `vh` are 1% of the page's
 * width and height.
 */
export function withStyleProp(
  base: TwStyle,
  style: StyleInput,
  parentSize: number,
  page: Units['page'],
): TwStyle {
  const list = (Array.isArray(style) ? style : [style]).filter((p): p is StyleProp => !!p);
  const outer: Units = { em: parentSize, page };
  // The font size comes first: `em` in every other length depends on it.
  let em = base.fontSize ?? parentSize;
  for (const p of list) {
    const size = toPt(p.fontSize, outer);
    if (typeof size === 'number') em = size;
  }
  const units: Units = { em, page };
  let s = base;
  for (const p of list) s = { ...s, ...fromStyleProp(p, units, outer) };
  for (const [key, v] of Object.entries(s)) {
    const bad = nonFinite(v);
    if (bad !== undefined) throw new Error(`${key}: ${bad} is not a finite number`);
  }
  return s;
}

/**
 * The first number in a style value that is not finite (`Infinity` from `100 / 0`, `NaN`). JSON
 * would turn it into `null`, which drops the property without a word.
 */
function nonFinite(v: unknown): number | undefined {
  if (typeof v === 'number') return Number.isFinite(v) ? undefined : v;
  if (typeof v !== 'object' || v === null) return undefined;
  for (const x of Object.values(v)) {
    const bad = nonFinite(x);
    if (bad !== undefined) return bad;
  }
  return undefined;
}

/**
 * Converts one style object to the flat style the builder uses. `fontUnits` resolve `fontSize`
 * (the parent's font size is its `em`); `units` resolve everything else.
 */
export function fromStyleProp(p: StyleProp, units?: Units, fontUnits = units): TwStyle {
  const len = (v: number | string) => toPt(v, units);
  const {
    color,
    backgroundColor,
    lineHeight,
    letterSpacing,
    padding,
    paddingTop,
    paddingRight,
    paddingBottom,
    paddingLeft,
    paddingHorizontal,
    paddingVertical,
    margin,
    marginTop,
    marginRight,
    marginBottom,
    marginLeft,
    marginHorizontal,
    marginVertical,
    inset,
    borderWidth,
    borderColor,
    borderStyle,
    borderRadius,
    border,
    borderTop,
    borderRight,
    borderBottom,
    borderLeft,
    fontFamily,
    gap,
    textDecoration,
    decorationColor,
    textDecorationColor,
    textDecorationStyle,
    textDecorationThickness,
    textOverflow,
    objectFit,
    objectPosition,
    translate,
    transform,
    transformOrigin,
    fontWeight,
    top,
    right,
    bottom,
    left,
    flex,
    ...rest
  } = p;
  const out: Record<string, unknown> = { ...rest };
  if (fontWeight !== undefined) out.fontWeight = weightOf(fontWeight);
  if (flex !== undefined) {
    const [grow, shrink, basis] = flexShorthand(flex, units);
    out.flexGrow ??= grow;
    out.flexShrink ??= shrink;
    out.flexBasis ??= basis;
  }
  if (textDecoration !== undefined) out.textDecoration = decorationLine(textDecoration);
  out.decorationStyle ??= textDecorationStyle;
  out.decorationThickness ??= textDecorationThickness;
  if (textOverflow !== undefined) out.ellipsis ??= overflowEllipsis(textOverflow);
  if (objectFit !== undefined) out.objectFit = fitOf(objectFit);
  if (objectPosition !== undefined) out.objectPosition = positionOf(objectPosition, units);
  for (const k of LENGTH_KEYS)
    if (out[k] !== undefined) out[k] = toPt(out[k] as string, k === 'fontSize' ? fontUnits : units);
  if (gap !== undefined) out.rowGap = out.columnGap = len(gap);
  if (color !== undefined) {
    const c = toColor(color);
    if (c !== 'currentColor') out.color = c;
  }
  if (backgroundColor !== undefined) out.backgroundColor = toColor(backgroundColor);
  const decoColor = decorationColor ?? textDecorationColor;
  if (decoColor !== undefined) {
    const c = toColor(decoColor);
    if (c !== 'currentColor') out.decorationColor = c;
  }
  if (lineHeight !== undefined) out.lineHeight = { mult: lineHeight };
  if (letterSpacing !== undefined) out.letterSpacing = { pt: len(letterSpacing) };
  if (fontFamily !== undefined) out.fontFamily = typeof fontFamily === 'string' ? [fontFamily] : fontFamily;
  spread(out, 'padding', '', padding, len);
  spread(out, 'margin', '', margin, len);
  // Axis and per-side keys are more specific than the shorthand: applied after it, in that order.
  sides(
    out,
    'padding',
    { x: paddingHorizontal, y: paddingVertical },
    {
      Top: paddingTop,
      Right: paddingRight,
      Bottom: paddingBottom,
      Left: paddingLeft,
    },
    units,
  );
  sides(
    out,
    'margin',
    { x: marginHorizontal, y: marginVertical },
    {
      Top: marginTop,
      Right: marginRight,
      Bottom: marginBottom,
      Left: marginLeft,
    },
    units,
  );
  spread(out, 'inset', '', inset, len);
  sides(out, 'inset', {}, { Top: top, Right: right, Bottom: bottom, Left: left }, units);
  spread(out, 'border', 'Width', borderWidth, len);
  spread<string | Color>(out, 'border', 'Color', borderColor, toColor);
  // Per-side keys (`borderBottomWidth`, …) arrive in `rest`; convert their units and colours here.
  for (const side of ['Top', 'Right', 'Bottom', 'Left'] as const) {
    const w = out[`border${side}Width`];
    if (w !== undefined) out[`border${side}Width`] = len(w as string | number);
    const c = out[`border${side}Color`];
    if (c !== undefined) out[`border${side}Color`] = toColor(c as string | Color);
  }
  if (typeof borderStyle === 'string') out.borderStyle = borderStyle;
  else spread(out, 'border', 'Style', borderStyle);
  // The shorthand strings fill what the keys above leave unset: a side first, then `border`.
  const sideBorders = { Top: borderTop, Right: borderRight, Bottom: borderBottom, Left: borderLeft };
  for (const [side, v] of Object.entries(sideBorders)) {
    if (v === undefined) continue;
    const b = borderShorthand(`border${side}`, v, units);
    out[`border${side}Width`] ??= b.width;
    if (b.color !== undefined) out[`border${side}Color`] ??= b.color;
    // A string `borderStyle` is a longhand for every side.
    if (b.style !== undefined && out.borderStyle === undefined) out[`border${side}Style`] ??= b.style;
  }
  if (border !== undefined) {
    const b = borderShorthand('border', border, units);
    for (const [, side] of EDGES) {
      out[`border${side}Width`] ??= b.width;
      if (b.color !== undefined) out[`border${side}Color`] ??= b.color;
    }
    if (b.style !== undefined) out.borderStyle ??= b.style;
  }
  if (typeof borderRadius === 'object') {
    const r = borderRadius;
    if (r.topLeft !== undefined) out.borderTopLeftRadius = len(r.topLeft);
    if (r.topRight !== undefined) out.borderTopRightRadius = len(r.topRight);
    if (r.bottomRight !== undefined) out.borderBottomRightRadius = len(r.bottomRight);
    if (r.bottomLeft !== undefined) out.borderBottomLeftRadius = len(r.bottomLeft);
    if (Object.keys(r).length < 4) out.borderRadius = 0;
  } else if (borderRadius !== undefined) {
    out.borderRadius = len(borderRadius);
  }
  if (translate !== undefined) out.translate = translate.map(len);
  if (typeof transform === 'string') out.transform = parseTransform('transform', transform);
  else if (transform !== undefined) out.transform = transform;
  if (typeof transformOrigin === 'string')
    out.transformOrigin = parseOrigin('transformOrigin', transformOrigin);
  else if (transformOrigin !== undefined) out.transformOrigin = transformOrigin.map(len);
  for (const k of Object.keys(out)) {
    if (out[k] === undefined) delete out[k];
    else if (!KNOWN_KEYS.has(k)) throw new Error(`unknown style property "${k}"`);
  }
  return out as TwStyle;
}

const WEIGHTS: Readonly<Record<string, number>> = {
  thin: 100,
  hairline: 100,
  ultralight: 200,
  extralight: 200,
  light: 300,
  normal: 400,
  regular: 400,
  medium: 500,
  semibold: 600,
  demibold: 600,
  bold: 700,
  ultrabold: 800,
  extrabold: 800,
  heavy: 900,
  black: 900,
};

/** A numeric weight, or a CSS name (`'bold'`). */
export function weightOf(w: number | string): number {
  const n = typeof w === 'number' ? w : WEIGHTS[w.toLowerCase()];
  if (n === undefined)
    throw new Error(`fontWeight "${w}" is not a number or a CSS weight name (bold, light, …)`);
  return n;
}

const KNOWN_KEYS: ReadonlySet<string> = new Set([
  ...BOX_KEYS,
  ...TEXT_INHERITED,
  ...TEXT_ONLY,
  'objectFit',
  'objectPosition',
]);

/** CSS `text-decoration-line`. A run has one line, so both lines draw the line-through. */
function decorationLine(v: string): TextDecoration {
  const lines = new Set(
    v
      .trim()
      .split(/\s+/)
      .map((l) => (l === 'lineThrough' ? 'line-through' : l)),
  );
  const known = ['none', 'underline', 'line-through'];
  if ([...lines].some((l) => !known.includes(l)) || (lines.has('none') && lines.size > 1)) {
    throw new Error(`textDecoration "${v}" is not none, underline, line-through or "underline line-through"`);
  }
  return lines.has('line-through') ? 'lineThrough' : lines.has('underline') ? 'underline' : 'none';
}

function overflowEllipsis(v: string): boolean {
  if (v !== 'ellipsis' && v !== 'clip') throw new Error(`textOverflow "${v}" is not ellipsis or clip`);
  return v === 'ellipsis';
}

const FITS: ReadonlySet<string> = new Set(['fill', 'contain', 'cover', 'none']);

function fitOf(v: string): ObjectFit {
  if (!FITS.has(v)) throw new Error(`objectFit "${v}" is not fill, contain, cover or none`);
  return v as ObjectFit;
}

/** CSS `object-position`: one or two keywords, lengths (in the style units) or percentages. */
function positionOf(v: unknown, units: Units | undefined): ObjectPosition {
  if (typeof v !== 'string') {
    throw new Error(`objectPosition must be a CSS string such as 'left top' or '25% 75%', not ${typeof v}`);
  }
  return parseObjectPosition('objectPosition', v, (part) => {
    if (part === '0') return 0;
    try {
      const pt = toPt(part, units);
      if (pt !== 'auto') return pt as LenPercent;
    } catch (e) {
      throw new Error(`objectPosition: ${v} (${(e as Error).message})`);
    }
    throw new Error(`objectPosition: ${v} (auto is not a position)`);
  });
}

const BORDER_STYLES: ReadonlySet<string> = new Set(['solid', 'dashed', 'dotted']);
/** CSS `thin`, `medium` and `thick`: 1px, 3px and 5px. */
const BORDER_WIDTHS = { thin: 0.75, medium: 2.25, thick: 3.75 } as const;

/** Splits a CSS value at spaces outside parentheses (`1px solid rgb(0 0 0)`). */
function words(v: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let word = '';
  for (const ch of v.trim()) {
    if (ch === '(') depth++;
    else if (ch === ')') depth--;
    if (depth === 0 && /\s/.test(ch)) {
      if (word) out.push(word);
      word = '';
    } else {
      word += ch;
    }
  }
  if (word) out.push(word);
  return out;
}

/**
 * CSS `border`: a width, a style and a colour in any order. A missing width is `medium`, a
 * missing style solid and a missing colour the text colour; `none` is no border.
 */
function borderShorthand(
  key: string,
  v: string | number,
  units: Units | undefined,
): { width: number; style?: BorderStyle; color?: ColorValue } {
  const fail = (): never => {
    throw new Error(
      `${key} "${v}" is not a border: use a width, a style (solid, dashed, dotted) and a colour, as in "1px solid #e5e7eb"`,
    );
  };
  if (typeof v === 'number') return v < 0 ? fail() : { width: v, style: 'solid', color: 'currentColor' };
  let width: number | undefined;
  let style: string | undefined;
  let color: ColorValue | undefined;
  for (const w of words(v)) {
    if (w === 'none' || w === 'hidden' || BORDER_STYLES.has(w)) {
      if (style !== undefined) fail();
      style = w;
    } else if (w in BORDER_WIDTHS || /^[-+]?\.?\d/.test(w)) {
      const pt = w in BORDER_WIDTHS ? BORDER_WIDTHS[w as keyof typeof BORDER_WIDTHS] : toPt(w, units);
      if (width !== undefined || typeof pt !== 'number' || pt < 0) fail();
      width = pt as number;
    } else {
      if (color !== undefined) fail();
      try {
        color = parseColor(w);
      } catch {
        fail();
      }
    }
  }
  if (style === 'none' || style === 'hidden') return { width: 0 };
  return {
    width: width ?? BORDER_WIDTHS.medium,
    style: (style ?? 'solid') as BorderStyle,
    color: color ?? 'currentColor',
  };
}

/** CSS `flex`: a number is `n 1 0%`; strings are `none`, `auto`, `initial` or `grow [shrink] [basis]`. */
function flexShorthand(v: number | string, units: Units | undefined): [number, number, number | string] {
  if (typeof v === 'number') return [v, 1, '0%'];
  const named = { none: [0, 0, 'auto'], auto: [1, 1, 'auto'], initial: [0, 1, 'auto'] } as const;
  if (v in named) return [...named[v as keyof typeof named]];
  const [g, s, b] = v.trim().split(/\s+/);
  const grow = Number(g);
  const shrink = s === undefined ? 1 : Number(s);
  if (!Number.isFinite(grow) || !Number.isFinite(shrink)) {
    throw new Error(`flex "${v}" is not a number or "grow shrink basis"`);
  }
  const basis = b === undefined ? '0%' : toPt(b, units);
  return [grow, shrink, basis];
}
