import type { Color, Style, TransformOp } from '@inkoxide/schema';
import {
  BORDER_STYLES,
  type BorderStyle,
  type ColorValue,
  type ObjectPosition,
  parseColor,
  parseFlex,
  parseObjectPosition,
  parseOrigin,
  parseTransform,
  parseFontFeatureSettings,
  parseFontVariantNumeric,
  round,
  RUN_ALIGNS,
  type RunAlign,
  splitTopLevel,
  TEXT_INHERITED,
  TEXT_ONLY,
  type TextDecoration,
  type FlatStyle,
  type Units,
} from '@inkoxide/style';
import { Reader } from '@inkoxide/style/internal';

import type { Edges, HyphenationCallback, StyleProp, Style as StyleInput } from '../components/index.ts';

/** A text property that cascades from an element to its content. */
type InheritedKey = (typeof TEXT_INHERITED)[number];

/** The keys of `TEXT_INHERITED` that are set at the top of every document. */
type RootKey = 'fontFamily' | 'fontWeight' | 'fontStyle' | 'fontSize' | 'color';

/**
 * The text properties in effect at a point of the tree, after inheritance: every
 * `TEXT_INHERITED` key, unset until an element sets one, apart from the font and the colour,
 * which the document sets.
 */
export interface TextCtx extends Omit<{ [K in InheritedKey]?: FlatStyle[K] }, RootKey> {
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
  /**
   * Where runs sit on their line (`verticalAlign` on a `Text`): it is not a `TEXT_INHERITED` key,
   * since a `Cell`'s `verticalAlign` must not reach its text, but the runs of a nested `Text` take
   * it from the `Text` around them.
   */
  verticalAlign?: RunAlign | undefined;
  /** Resolved font id, cached by the builder (reset when the font properties change). */
  fontId?: string | undefined;
}

const inheritedFlag = new WeakMap<FlatStyle, boolean>();

/** True when the style sets any inherited text property. Cached per (immutable) style object. */
export function hasInherited(s: FlatStyle): boolean {
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
  verticalAlign?: RunAlign | undefined;
  direction?: FlatStyle['direction'];
  orphans?: number | undefined;
  widows?: number | undefined;
}

/** The context of an element's content: the parent's, overridden by the element's own style. */
export function inherit(parent: TextCtx, own: FlatStyle, extra?: InheritExtra): TextCtx {
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
    if (extra.verticalAlign !== undefined) c.verticalAlign = extra.verticalAlign;
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

/** `verticalAlign` when it places a run (`super`, `sub`, `baseline`), not a cell's content. */
export function runAlignOf(s: FlatStyle): RunAlign | undefined {
  const v = s.verticalAlign;
  return typeof v === 'string' && (RUN_ALIGNS as readonly string[]).includes(v) ? (v as RunAlign) : undefined;
}

/** `verticalAlign`'s values: a cell's, then a run's, then those that place an image in text. */
const VERTICAL_ALIGNS: readonly string[] = [
  'top',
  'middle',
  'bottom',
  ...RUN_ALIGNS,
  'textTop',
  'textBottom',
  'text-top',
  'text-bottom',
];

/** A colour, with `currentColor` as `current`. */
export const resolveColor = (c: ColorValue, current: Color): Color => (c === 'currentColor' ? current : c);

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
] as const satisfies ReadonlyArray<keyof FlatStyle & keyof Style>;

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
export function boxKeys(s: FlatStyle): string[] {
  return Object.keys(s).filter((k) => BOX_KEYS.has(k) && s[k as keyof FlatStyle] !== undefined);
}

export function textOnlyKeys(s: FlatStyle): string[] {
  return Object.keys(s).filter((k) => TEXT_ONLY_KEYS.has(k) && s[k as keyof FlatStyle] !== undefined);
}

type Edge = 'top' | 'right' | 'bottom' | 'left';
const EDGES: ReadonlyArray<[Edge, 'Top' | 'Right' | 'Bottom' | 'Left']> = [
  ['top', 'Top'],
  ['right', 'Right'],
  ['bottom', 'Bottom'],
  ['left', 'Left'],
];

type EdgeKeys = readonly [Edge, keyof FlatStyle][];
const keysFor = (prefix: string, suffix = ''): EdgeKeys =>
  EDGES.map(
    ([edge, cap]) => [edge, `${prefix}${cap}${suffix}` as keyof FlatStyle] as [Edge, keyof FlatStyle],
  );
const PADDING = keysFor('padding');
const MARGIN = keysFor('margin');
const INSET = keysFor('inset');
const BORDER_WIDTH = keysFor('border', 'Width');
const BORDER_COLOR = keysFor('border', 'Color');
const BORDER_STYLE = keysFor('border', 'Style');

function edgeObject<T>(s: FlatStyle, keys: EdgeKeys): Partial<Record<Edge, T>> | undefined {
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

const plans = new WeakMap<FlatStyle, BoxPlan>();
const cellPlans = new WeakMap<FlatStyle, BoxPlan>();

function usesCurrentColor(s: FlatStyle): boolean {
  if (s.backgroundColor === 'currentColor') return true;
  for (let i = 0; i < 4; i++) {
    const w = s[BORDER_WIDTH[i]?.[1] as keyof FlatStyle] as number | undefined;
    const c = s[BORDER_COLOR[i]?.[1] as keyof FlatStyle];
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
export function boxStyle(s: FlatStyle, currentColor: Color, cell = false): Style | undefined {
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

function computeBox(s: FlatStyle, currentColor: Color, cell: boolean): Style | undefined {
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
    const [edge, ck] = BORDER_COLOR[i] as [Edge, keyof FlatStyle];
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
  const sideStyles = edgeObject<NonNullable<FlatStyle['borderStyle']>>(s, BORDER_STYLE);
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
  // A run's `verticalAlign` (`super`) is the tree builder's, and an error on a cell.
  if (cell && s.verticalAlign !== undefined && runAlignOf(s) === undefined) {
    out.verticalAlign = s.verticalAlign as Style['verticalAlign'];
    any = true;
  }
  return any ? out : undefined;
}

/** Adds the text-node properties (alignment, line height, clamping, hyphenation) to a style. */
export function textNodeStyle(
  base: Style | undefined,
  own: FlatStyle,
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
  if (ctx.textWrap === 'pretty') set('textWrap', 'pretty');
  // A hyphenation callback replaces the patterns: the engine would break the words it keeps whole.
  if (usesPatterns(ctx)) set('hyphenate', true);
  if (ctx.lang !== undefined && ctx.lang !== docLang) set('lang', ctx.lang);
  return out;
}

// ---------------------------------------------------------------------------------------------
// The `style` prop

function toColor(v: string | Color): ColorValue {
  return typeof v === 'string' ? parseColor(v) : v;
}

/**
 * Writes an edge value, one for every side or `{ top, right, … }`, to the side keys
 * `<prefix><Side><suffix>` (`paddingTop`, `borderLeftColor`), through `map`.
 */
function setEdges<T>(
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

/**
 * A length of the style key `prop`: a number is points, and a string is read as a class's value
 * is (`'20mm'`, `'5PX'`, `'calc(100% - 10pt)'`), with `em`, `vw` and `vh` in `units`. `kind` is
 * what the key takes: `len` a length, a percentage or `auto`, `lenPercent` no `auto`, and
 * `points` neither. Other values pass through, for validation to report.
 */
function lengthOf<T>(kind: 'len' | 'lenPercent' | 'points', prop: string, v: T, units?: Units) {
  return typeof v === 'string' ? new Reader(prop, units)[kind](v) : v;
}

/** Points from a length prop (`12`, `'20mm'`); `em`, `vw` and `vh` need `units`. */
export function toPt(prop: string, v: number | string, units?: Units): number {
  return typeof v === 'number' ? v : new Reader(prop, units).points(v);
}

/** The keys that take one length, and what kind: see `lengthOf`. */
const LENGTH_KEYS = [
  ['width', 'len'],
  ['height', 'len'],
  ['minWidth', 'len'],
  ['minHeight', 'len'],
  ['maxWidth', 'len'],
  ['maxHeight', 'len'],
  ['flexBasis', 'len'],
  ['rowGap', 'lenPercent'],
  ['columnGap', 'lenPercent'],
  ['borderSpacing', 'points'],
  ['textIndent', 'points'],
  ['decorationThickness', 'points'],
] as const;

/**
 * Writes the axis keys (`paddingHorizontal` to Left and Right, `paddingVertical` to Top and
 * Bottom), then the per-side keys, converting their units.
 */
function setAxesAndSides(
  out: Record<string, unknown>,
  prop: 'padding' | 'margin' | 'inset',
  axis: { x?: string | number; y?: string | number },
  each: Record<'Top' | 'Right' | 'Bottom' | 'Left', string | number | undefined>,
  units: Units | undefined,
): void {
  // Padding takes no `auto`.
  const kind = prop === 'padding' ? 'lenPercent' : 'len';
  const set = (side: string, v: string | number | undefined) => {
    if (v !== undefined) out[`${prop}${side}`] = lengthOf(kind, `${prop}${side}`, v, units);
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
  base: FlatStyle,
  style: StyleInput,
  parentSize: number,
  page: Units['page'],
): FlatStyle {
  const list = styleList(style);
  const outer: Units = { em: parentSize, page };
  // The font size comes first: `em` in every other length depends on it.
  const units: Units = { em: styleFontSize(style, parentSize, page) ?? base.fontSize ?? parentSize, page };
  let s = base;
  for (const p of list) s = over(s, fromStyleProp(p, units, outer));
  for (const [key, v] of Object.entries(s)) {
    const bad = nonFinite(v);
    if (bad !== undefined) throw new Error(`${key}: ${bad} is not a finite number`);
  }
  return s;
}

function styleList(style: StyleInput): StyleProp[] {
  return (Array.isArray(style) ? style : [style]).filter((p): p is StyleProp => !!p);
}

/** The font size a `style` prop sets, in points: `em` in it is `parentSize`. */
export function styleFontSize(
  style: StyleInput,
  parentSize: number,
  page: Units['page'],
): number | undefined {
  let size: number | undefined;
  for (const p of styleList(style)) {
    const v = lengthOf('points', 'fontSize', p.fontSize, { em: parentSize, page });
    if (typeof v === 'number') size = v;
  }
  return size;
}

/** Keys for every side or corner at once, and the per-side or per-corner keys they replace. */
const ALL_SIDES: ReadonlyArray<readonly [keyof FlatStyle, ReadonlyArray<keyof FlatStyle>]> = [
  ['borderStyle', BORDER_STYLE.map(([, k]) => k)],
  ['borderRadius', CORNER_KEYS.map(([, k]) => k)],
];

/**
 * `next` over `s`, as a later CSS rule: a `borderStyle` or `borderRadius` for every side or
 * corner replaces the per-side and per-corner values `s` has, which would otherwise win.
 */
function over(s: FlatStyle, next: FlatStyle): FlatStyle {
  const out: Record<string, unknown> = { ...s, ...next };
  for (const [all, each] of ALL_SIDES) {
    if (next[all] === undefined) continue;
    for (const k of each) if (next[k] === undefined) delete out[k];
  }
  return out as FlatStyle;
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
export function fromStyleProp(p: StyleProp, units?: Units, fontUnits = units): FlatStyle {
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
    fontFeatureSettings,
    fontVariantNumeric,
    verticalAlign,
    textWrap,
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
  for (const k of ['flexGrow', 'flexShrink'] as const) if (out[k] !== undefined) checkFactor(k, out[k]);
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
  if (objectFit !== undefined) out.objectFit = new Reader('objectFit').oneOf('objectFit', String(objectFit));
  if (objectPosition !== undefined) out.objectPosition = positionOf(objectPosition, units);
  if (fontFeatureSettings !== undefined)
    out.fontFeatureSettings = parseFontFeatureSettings('fontFeatureSettings', fontFeatureSettings);
  if (fontVariantNumeric !== undefined)
    out.fontVariantNumeric = parseFontVariantNumeric('fontVariantNumeric', fontVariantNumeric);
  if (textWrap !== undefined) {
    if (textWrap === 'balance') {
      throw new Error(`textWrap "balance" is not supported yet: use 'pretty' for even lines, or 'wrap'`);
    }
    out.textWrap = new Reader('textWrap').oneOf('textWrap', String(textWrap));
  }
  if (verticalAlign !== undefined) {
    // A number (points up) or a CSS keyword; an `<Image>` in a `<Text>` takes them all.
    const v = verticalAlign as unknown;
    if (typeof v === 'number') {
      if (!Number.isFinite(v)) throw new Error(`verticalAlign ${v} is not a finite number of points`);
      out.verticalAlign = v as FlatStyle['verticalAlign'];
    } else if (typeof v === 'string' && VERTICAL_ALIGNS.includes(v)) {
      const camel = { 'text-top': 'textTop', 'text-bottom': 'textBottom' }[v] ?? v;
      out.verticalAlign = camel as FlatStyle['verticalAlign'];
    } else {
      throw new Error(
        `verticalAlign "${String(v)}" is not supported: use 'super', 'sub' or 'baseline' on a Text, 'top', 'middle' or 'bottom' on a Cell, and on an Image in a Text also 'text-top', 'text-bottom' or a number of points`,
      );
    }
  }
  if (out.fontSize !== undefined) out.fontSize = lengthOf('points', 'fontSize', out.fontSize, fontUnits);
  for (const [k, kind] of LENGTH_KEYS) if (out[k] !== undefined) out[k] = lengthOf(kind, k, out[k], units);
  if (gap !== undefined) out.rowGap = out.columnGap = lengthOf('lenPercent', 'gap', gap, units);
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
  if (letterSpacing !== undefined)
    out.letterSpacing = { pt: lengthOf('points', 'letterSpacing', letterSpacing, units) };
  if (fontFamily !== undefined) out.fontFamily = typeof fontFamily === 'string' ? [fontFamily] : fontFamily;
  setEdges(out, 'padding', '', padding, (v) => lengthOf('lenPercent', 'padding', v, units));
  setEdges(out, 'margin', '', margin, (v) => lengthOf('len', 'margin', v, units));
  // Axis and per-side keys are more specific than the shorthand: applied after it, in that order.
  setAxesAndSides(
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
  setAxesAndSides(
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
  setEdges(out, 'inset', '', inset, (v) => lengthOf('len', 'inset', v, units));
  setAxesAndSides(out, 'inset', {}, { Top: top, Right: right, Bottom: bottom, Left: left }, units);
  setEdges(out, 'border', 'Width', borderWidth, (v) => lengthOf('points', 'borderWidth', v, units));
  setEdges<string | Color>(out, 'border', 'Color', borderColor, toColor);
  // Per-side keys (`borderBottomWidth`, …) arrive in `rest`; convert their units and colours here.
  for (const side of ['Top', 'Right', 'Bottom', 'Left'] as const) {
    const w = out[`border${side}Width`];
    if (w !== undefined) out[`border${side}Width`] = lengthOf('points', `border${side}Width`, w, units);
    const c = out[`border${side}Color`];
    if (c !== undefined) out[`border${side}Color`] = toColor(c as string | Color);
  }
  if (typeof borderStyle === 'string') out.borderStyle = borderStyle;
  else setEdges(out, 'border', 'Style', borderStyle);
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
  const radius = (v: number | string) =>
    typeof v === 'string' ? new Reader('borderRadius', units).radius(v) : v;
  if (typeof borderRadius === 'object') {
    const r = borderRadius;
    if (r.topLeft !== undefined) out.borderTopLeftRadius = radius(r.topLeft);
    if (r.topRight !== undefined) out.borderTopRightRadius = radius(r.topRight);
    if (r.bottomRight !== undefined) out.borderBottomRightRadius = radius(r.bottomRight);
    if (r.bottomLeft !== undefined) out.borderBottomLeftRadius = radius(r.bottomLeft);
  } else if (borderRadius !== undefined) {
    out.borderRadius = radius(borderRadius);
  }
  if (translate !== undefined)
    out.translate = translate.map((v) => lengthOf('lenPercent', 'translate', v, units));
  if (typeof transform === 'string') out.transform = parseTransform('transform', transform, units);
  else if (transform !== undefined) out.transform = transform;
  if (typeof transformOrigin === 'string')
    out.transformOrigin = parseOrigin('transformOrigin', transformOrigin, units);
  else if (transformOrigin !== undefined)
    out.transformOrigin = transformOrigin.map((v) => lengthOf('lenPercent', 'transformOrigin', v, units));
  for (const k of Object.keys(out)) {
    if (out[k] === undefined) delete out[k];
    else if (!KNOWN_KEYS.has(k)) throw new Error(`unknown style property "${k}"`);
  }
  return out as FlatStyle;
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
  const name = typeof w === 'string' ? w.toLowerCase() : '';
  const n = typeof w === 'number' ? w : Object.hasOwn(WEIGHTS, name) ? WEIGHTS[name] : undefined;
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

/** CSS `text-decoration-line`, and `lineThrough`, the schema's spelling of `line-through`. */
function decorationLine(v: string): TextDecoration {
  return new Reader('textDecoration').decorationLine(String(v).replace(/\blineThrough\b/g, 'line-through'));
}

function overflowEllipsis(v: string): boolean {
  if (v !== 'ellipsis' && v !== 'clip') throw new Error(`textOverflow "${v}" is not ellipsis or clip`);
  return v === 'ellipsis';
}

/** CSS `object-position`: one or two keywords, lengths (in the style units) or percentages. */
function positionOf(v: unknown, units: Units | undefined): ObjectPosition {
  if (typeof v !== 'string') {
    throw new Error(`objectPosition must be a CSS string such as 'left top' or '25% 75%', not ${typeof v}`);
  }
  const r = new Reader('objectPosition', units);
  return parseObjectPosition('objectPosition', v, (part) => r.lenPercent(part));
}

/** CSS `thin`, `medium` and `thick`: 1px, 3px and 5px. */
const BORDER_WIDTHS = { thin: 0.75, medium: 2.25, thick: 3.75 } as const;

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
  for (const w of splitTopLevel(v, ' ')) {
    if (w === 'none' || w === 'hidden' || (BORDER_STYLES as readonly string[]).includes(w)) {
      if (style !== undefined) fail();
      style = w;
    } else if (w in BORDER_WIDTHS || /^[-+]?\.?\d/.test(w)) {
      let pt: number | undefined;
      try {
        pt = w in BORDER_WIDTHS ? BORDER_WIDTHS[w as keyof typeof BORDER_WIDTHS] : toPt(key, w, units);
      } catch {
        fail();
      }
      if (width !== undefined || pt === undefined || pt < 0) fail();
      width = pt;
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

/** `flexGrow` or `flexShrink`: a number that is not negative, as `flex` takes them. */
function checkFactor(key: string, v: unknown): void {
  if (typeof v !== 'number') throw new Error(`${key} ${JSON.stringify(v)} is not a number`);
  if (v < 0) throw new Error(`${key} ${v}: cannot be negative`);
}

/** CSS `flex`: a number is `n 1 0%`; strings are read by `parseFlex`, with plain numbers for grow and shrink. */
function flexShorthand(v: number | string, units: Units | undefined): [number, number, number | string] {
  if (typeof v === 'number') {
    if (v < 0) throw new Error(`flex ${v}: grow and shrink cannot be negative`);
    return [v, 1, '0%'];
  }
  const number = (x: string): number => {
    const n = Number(x);
    if (!Number.isFinite(n)) throw new Error(`flex "${v}" is not a number or "grow shrink basis"`);
    return n;
  };
  return parseFlex(v, number, (b) => lengthOf('len', 'flex', b, units));
}
