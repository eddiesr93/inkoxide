import type { Color, Style, TransformOp } from '@inkoxide/schema';
import {
  type ColorValue,
  type LetterSpacing,
  type LineHeight,
  parseColor,
  parseOrigin,
  parseTransform,
  TEXT_INHERITED,
  TEXT_ONLY,
  type TwStyle,
} from '@inkoxide/tailwind';

import type { StyleProp } from '../components/index.ts';

/** The text properties in effect at a point of the tree, after inheritance. */
export interface TextCtx {
  fontFamily: readonly string[];
  fontWeight: number;
  fontStyle: 'normal' | 'italic';
  fontSize: number;
  color: Color;
  lineHeight: LineHeight | undefined;
  letterSpacing: LetterSpacing | undefined;
  textAlign: TwStyle['textAlign'];
  textDecoration: TwStyle['textDecoration'];
  textTransform: TwStyle['textTransform'];
  whiteSpace: TwStyle['whiteSpace'];
  hyphenate: boolean | undefined;
  decorationColor: Color | undefined;
  decorationThickness: number | undefined;
  decorationStyle: TwStyle['decorationStyle'];
  textIndent: number | undefined;
  direction: TwStyle['direction'];
  orphans: number | undefined;
  widows: number | undefined;
  lang: string | undefined;
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
    if (own.fontFamily !== undefined || own.fontWeight !== undefined || own.fontStyle !== undefined) {
      c.fontId = undefined;
      if (own.fontFamily !== undefined) c.fontFamily = own.fontFamily;
      if (own.fontWeight !== undefined) c.fontWeight = own.fontWeight;
      if (own.fontStyle !== undefined) c.fontStyle = own.fontStyle;
    }
    if (own.fontSize !== undefined) c.fontSize = own.fontSize;
    if (own.color !== undefined) c.color = own.color;
    if (own.lineHeight !== undefined) c.lineHeight = own.lineHeight;
    if (own.letterSpacing !== undefined) c.letterSpacing = own.letterSpacing;
    if (own.textAlign !== undefined) c.textAlign = own.textAlign;
    if (own.textDecoration !== undefined) c.textDecoration = own.textDecoration;
    if (own.textTransform !== undefined) c.textTransform = own.textTransform;
    if (own.whiteSpace !== undefined) c.whiteSpace = own.whiteSpace;
    if (own.hyphenate !== undefined) c.hyphenate = own.hyphenate;
    if (own.decorationColor !== undefined) c.decorationColor = own.decorationColor;
    if (own.decorationThickness !== undefined) c.decorationThickness = own.decorationThickness;
    if (own.decorationStyle !== undefined) c.decorationStyle = own.decorationStyle;
    if (own.textIndent !== undefined) c.textIndent = own.textIndent;
    if (own.direction !== undefined) c.direction = own.direction;
    if (own.orphans !== undefined) c.orphans = own.orphans;
    if (own.widows !== undefined) c.widows = own.widows;
  }
  if (extra) {
    if (extra.lang !== undefined) c.lang = extra.lang;
    if (extra.hyphenate !== undefined) c.hyphenate = extra.hyphenate;
    if (extra.link !== undefined) c.link = extra.link;
    if (extra.direction !== undefined) c.direction = extra.direction;
    if (extra.orphans !== undefined) c.orphans = extra.orphans;
    if (extra.widows !== undefined) c.widows = extra.widows;
  }
  return c;
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
  if (ctx.hyphenate) set('hyphenate', true);
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

const PT_PER: Record<string, number> = { pt: 1, mm: 72 / 25.4, cm: 720 / 25.4, in: 72, px: 0.75 };

/** Points from a length: a number, or a string with a unit (`'20mm'`). Percentages and `auto` pass through. */
export function toPt<T>(v: number | string | T): number | T | string {
  if (typeof v !== 'string') return v;
  if (v === 'auto' || /^-?(\d+\.?\d*|\.\d+)%$/.test(v)) return v;
  const m = /^(-?(?:\d+\.?\d*|\.\d+))\s*(pt|mm|cm|in|px)$/.exec(v.trim());
  if (!m?.[1] || !m[2])
    throw new Error(`"${v}" is not a length (use a number of points or a unit: pt, mm, cm, in, px)`);
  return round(Number(m[1]) * (PT_PER[m[2]] ?? 1));
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

/** Converts the friendly `style` prop to the flat style the builder uses. */
function sides(
  out: Record<string, unknown>,
  prop: 'padding' | 'margin' | 'inset',
  axis: { x?: string | number; y?: string | number },
  each: Record<'Top' | 'Right' | 'Bottom' | 'Left', string | number | undefined>,
): void {
  const set = (side: string, v: string | number | undefined) => {
    if (v !== undefined) out[`${prop}${side}`] = v === 'auto' ? 'auto' : toPt(v);
  };
  set('Left', axis.x);
  set('Right', axis.x);
  set('Top', axis.y);
  set('Bottom', axis.y);
  for (const [side, v] of Object.entries(each)) set(side, v);
}

export function fromStyleProp(p: StyleProp): TwStyle {
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
    fontFamily,
    gap,
    decorationColor,
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
    const [grow, shrink, basis] = flexShorthand(flex);
    out.flexGrow ??= grow;
    out.flexShrink ??= shrink;
    out.flexBasis ??= basis;
  }
  for (const k of LENGTH_KEYS) if (out[k] !== undefined) out[k] = toPt(out[k] as string);
  if (gap !== undefined) out.rowGap = out.columnGap = toPt(gap);
  if (color !== undefined) {
    const c = toColor(color);
    if (c !== 'currentColor') out.color = c;
  }
  if (backgroundColor !== undefined) out.backgroundColor = toColor(backgroundColor);
  if (decorationColor !== undefined) {
    const c = toColor(decorationColor);
    if (c !== 'currentColor') out.decorationColor = c;
  }
  if (lineHeight !== undefined) out.lineHeight = { mult: lineHeight };
  if (letterSpacing !== undefined) out.letterSpacing = { pt: toPt(letterSpacing) };
  if (fontFamily !== undefined) out.fontFamily = typeof fontFamily === 'string' ? [fontFamily] : fontFamily;
  spread(out, 'padding', '', padding, toPt);
  spread(out, 'margin', '', margin, toPt);
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
  );
  spread(out, 'inset', '', inset, toPt);
  sides(out, 'inset', {}, { Top: top, Right: right, Bottom: bottom, Left: left });
  spread(out, 'border', 'Width', borderWidth, toPt);
  spread<string | Color>(out, 'border', 'Color', borderColor, toColor);
  // Per-side keys (`borderBottomWidth`, …) arrive in `rest`; convert their units and colours here.
  for (const side of ['Top', 'Right', 'Bottom', 'Left'] as const) {
    const w = out[`border${side}Width`];
    if (w !== undefined) out[`border${side}Width`] = toPt(w as string | number);
    const c = out[`border${side}Color`];
    if (c !== undefined) out[`border${side}Color`] = toColor(c as string | Color);
  }
  if (typeof borderStyle === 'string') out.borderStyle = borderStyle;
  else spread(out, 'border', 'Style', borderStyle);
  if (typeof borderRadius === 'object') {
    const r = borderRadius;
    if (r.topLeft !== undefined) out.borderTopLeftRadius = toPt(r.topLeft);
    if (r.topRight !== undefined) out.borderTopRightRadius = toPt(r.topRight);
    if (r.bottomRight !== undefined) out.borderBottomRightRadius = toPt(r.bottomRight);
    if (r.bottomLeft !== undefined) out.borderBottomLeftRadius = toPt(r.bottomLeft);
    if (Object.keys(r).length < 4) out.borderRadius = 0;
  } else if (borderRadius !== undefined) {
    out.borderRadius = toPt(borderRadius);
  }
  if (translate !== undefined) out.translate = translate.map((x) => toPt(x));
  if (typeof transform === 'string') out.transform = parseTransform('transform', transform);
  else if (transform !== undefined) out.transform = transform;
  if (typeof transformOrigin === 'string')
    out.transformOrigin = parseOrigin('transformOrigin', transformOrigin);
  else if (transformOrigin !== undefined) out.transformOrigin = transformOrigin.map((x) => toPt(x));
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

const KNOWN_KEYS: ReadonlySet<string> = new Set([...BOX_KEYS, ...TEXT_INHERITED, ...TEXT_ONLY]);

/** CSS `flex`: a number is `n 1 0%`; strings are `none`, `auto`, `initial` or `grow [shrink] [basis]`. */
function flexShorthand(v: number | string): [number, number, number | string] {
  if (typeof v === 'number') return [v, 1, '0%'];
  const named = { none: [0, 0, 'auto'], auto: [1, 1, 'auto'], initial: [0, 1, 'auto'] } as const;
  if (v in named) return [...named[v as keyof typeof named]];
  const [g, s, b] = v.trim().split(/\s+/);
  const grow = Number(g);
  const shrink = s === undefined ? 1 : Number(s);
  if (!Number.isFinite(grow) || !Number.isFinite(shrink)) {
    throw new Error(`flex "${v}" is not a number or "grow shrink basis"`);
  }
  const basis = b === undefined ? '0%' : toPt(b);
  return [grow, shrink, basis];
}
