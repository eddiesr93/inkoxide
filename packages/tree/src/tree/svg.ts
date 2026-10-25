import type { Color, FlatStyle, FontStretch } from '@inkoxide/style';
import { parseFontFamily } from '@inkoxide/style/internal';

import { type DrawingFont, escapeXml, quotedFamily } from '../canvas.ts';
import { HOST, HOST_NAME, type SvgPresentation, type SvgProps } from '../components/index.ts';
import type { Instance, Node } from '../nodes.ts';
import { stretchOf, weightOf } from './style.ts';

/**
 * SVG element name of each primitive host type. The reconciler lets these, and nothing else, into
 * an `<Svg>`.
 */
export const SVG_TAGS: Readonly<Record<string, string>> = {
  [HOST.G]: 'g',
  [HOST.Path]: 'path',
  [HOST.Rect]: 'rect',
  [HOST.Circle]: 'circle',
  [HOST.Ellipse]: 'ellipse',
  [HOST.Line]: 'line',
  [HOST.Polyline]: 'polyline',
  [HOST.Polygon]: 'polygon',
  [HOST.Defs]: 'defs',
  [HOST.ClipPath]: 'clipPath',
  [HOST.Marker]: 'marker',
  [HOST.Tspan]: 'tspan',
  [HOST.TextPath]: 'textPath',
  [HOST.LinearGradient]: 'linearGradient',
  [HOST.RadialGradient]: 'radialGradient',
  [HOST.Stop]: 'stop',
};

/** Attributes SVG spells in camelCase; every other camelCase prop becomes kebab-case. */
const CAMEL_ATTRS = new Set([
  'viewBox',
  'preserveAspectRatio',
  'gradientUnits',
  'gradientTransform',
  'clipPathUnits',
  'pathLength',
  'refX',
  'refY',
  'markerWidth',
  'markerHeight',
  'markerUnits',
  'textLength',
  'lengthAdjust',
  'startOffset',
]);

/** The props `<Svg>` adds to the SVG presentation attributes, apart from its own SVG ones. */
type SvgBoxProp = Exclude<keyof SvgProps, keyof SvgPresentation | 'viewBox' | 'preserveAspectRatio'>;

/**
 * Props of `<Svg>` that are about its box in the layout, not the drawing. Keyed by `SvgProps`, so
 * a prop added there does not compile until it is listed here or excluded above as an SVG one.
 */
const BOX_KEYS: Record<SvgBoxProp, true> = {
  width: true,
  height: true,
  className: true,
  style: true,
  pages: true,
  break: true,
  href: true,
  fixed: true,
  debug: true,
  alt: true,
  role: true,
};
/**
 * Props the builder reads from `<Svg>` for its node, beyond `SvgProps`: the aliases of `break`,
 * and `bookmark`, as on a `View`.
 */
const FLOW_ALIASES = ['bookmark', 'wrap', 'minPresenceAhead'];
const BOX_PROPS: ReadonlySet<string> = new Set([...Object.keys(BOX_KEYS), ...FLOW_ALIASES, 'children']);

/**
 * Props of an element in the layout, which an element inside an `<Svg>` has not: written as SVG
 * attributes, they would do nothing. `href` is left to SVG, where a gradient takes it.
 */
const LAYOUT_ONLY = ['pages', 'break', 'fixed', 'debug', 'render', ...FLOW_ALIASES];

const attrName = (k: string): string =>
  CAMEL_ATTRS.has(k) ? k : k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

/** The attributes of `props`, by name: the same markup whatever order a front end set them in. */
function attrs(props: Record<string, unknown>, skip: ReadonlySet<string>): string {
  let out = '';
  for (const k of Object.keys(props).sort()) {
    const v = props[k];
    if (skip.has(k) || v === undefined || v === null || v === false) continue;
    const value = Array.isArray(v) ? v.join(' ') : String(v);
    out += ` ${attrName(k)}="${escapeXml(value)}"`;
  }
  return out;
}

const CHILD_SKIP = new Set(['children']);
/** The font props of SVG text, which it writes as the face they resolve to. */
const TEXT_SKIP = new Set(['children', 'fontFamily', 'fontWeight', 'fontStyle', 'fontStretch']);

/** What the text of a drawing needs from the tree builder. */
export interface SvgTextFonts {
  /** The text style around the drawing, which its text inherits. */
  font: DrawingFont;
  /** Its font size in points. */
  size: number;
  /** Its text colour, which `currentColor` in the drawing is. */
  color: Color;
  /** The registered face for `font` (marked as used): its family, weight, style and width. */
  face(font: DrawingFont, inst: Instance): DrawingFont;
  /** The style of the classes `className` of `inst`. */
  classes(inst: Instance, className: string): FlatStyle;
  fail(inst: Instance, message: string): never;
}

/** `font` with the font props `props` set. */
function fontOf(font: DrawingFont, props: Record<string, unknown>): DrawingFont {
  const { fontFamily, fontWeight, fontStyle, fontStretch } = props;
  const out: DrawingFont = {
    family: typeof fontFamily === 'string' ? parseFontFamily(fontFamily) : font.family,
    weight: fontWeight === undefined ? font.weight : weightOf(fontWeight as number | string),
    style: fontStyle === 'italic' ? 'italic' : fontStyle === 'normal' ? 'normal' : font.style,
  };
  const stretch = fontStretch === undefined ? font.stretch : stretchOf(fontStretch as FontStretch);
  if (stretch !== undefined) out.stretch = stretch;
  return out;
}

/** The keys a `style` object takes: the SVG attributes CSS can also set. */
const STYLE_KEYS: ReadonlySet<string> = new Set([
  'fill',
  'fillOpacity',
  'fillRule',
  'stroke',
  'strokeWidth',
  'strokeOpacity',
  'strokeLinecap',
  'strokeLinejoin',
  'strokeDasharray',
  'strokeDashoffset',
  'strokeMiterlimit',
  'opacity',
  'transform',
  'clipPath',
  'clipRule',
  'markerStart',
  'markerMid',
  'markerEnd',
  'visibility',
  'color',
  'fontFamily',
  'fontSize',
  'fontWeight',
  'fontStyle',
  'fontStretch',
  'textAnchor',
  'dominantBaseline',
  'letterSpacing',
  'wordSpacing',
  'textDecoration',
  'direction',
  'writingMode',
  'textOrientation',
  'fontFeatureSettings',
  'fontVariant',
  'fontKerning',
  'baselineShift',
  'alignmentBaseline',
]);

/** `c` as an SVG colour. */
function svgColor(c: Color): string {
  const [r, g, b] = c.slice(0, 3).map((v) => Math.round(v * 255));
  const a = Math.round((c[3] ?? 1) * 1000) / 1000;
  return a === 1 ? `rgb(${r},${g},${b})` : `rgba(${r},${g},${b},${a})`;
}

const paint = (v: unknown): string =>
  v === 'none' || v === 'currentColor' ? (v as string) : svgColor(v as Color);
const same = (v: unknown): unknown => v;

/** What each key of a class style is as an SVG attribute (keyed by its prop name). */
const CLASS_SVG: Readonly<Record<string, (v: unknown) => unknown>> = {
  fill: paint,
  stroke: paint,
  strokeWidth: same,
  opacity: same,
  color: (v) => svgColor(v as Color),
  fontFamily: (v) => (v as string[]).map(quotedFamily).join(', '),
  fontSize: same,
  fontWeight: same,
  fontStyle: same,
  fontStretch: same,
  letterSpacing: (v) => {
    const l = v as { pt: number } | { em: number };
    return 'em' in l ? `${l.em}em` : l.pt;
  },
  textDecoration: (v) => (v === 'lineThrough' ? 'line-through' : v),
  // Set with the size by `text-*`; SVG text is one line.
  lineHeight: () => undefined,
};

/** What the classes of `n` set, as SVG props. */
function classProps(n: Instance, className: string, text: SvgTextFonts): Record<string, unknown> {
  const s = text.classes(n, className);
  const bad = Object.keys(s).filter((k) => CLASS_SVG[k] === undefined);
  if (bad.length > 0) {
    text.fail(
      n,
      `className="${className}" on <${HOST_NAME[n.type]}> inside <Svg>: ${bad.join(', ')} ${bad.length === 1 ? 'does' : 'do'} not apply to a drawing, which takes fill-*, stroke-*, stroke-<n>, opacity-*, a text colour and font utilities.`,
    );
  }
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(s)) out[k] = CLASS_SVG[k]?.(v);
  return out;
}

/**
 * The props of `n` as SVG attributes: its own, then what its `className` sets over them, then its
 * `style`, as CSS applies them.
 */
function ownProps(n: Instance, text: SvgTextFonts): Record<string, unknown> {
  const { className, style, ...props } = n.props;
  const name = HOST_NAME[n.type];
  if (typeof className === 'string' && className.trim() !== '') {
    Object.assign(props, classProps(n, className, text));
  } else if (className !== undefined && typeof className !== 'string') {
    text.fail(n, `className on <${name}> inside <Svg> is a string of classes.`);
  }
  if (style === undefined || style === null) return props;
  if (typeof style !== 'object' || Array.isArray(style)) {
    text.fail(n, `style on <${name}> inside <Svg> is an object of SVG attributes ({ fill: 'teal' }).`);
  }
  for (const [k, v] of Object.entries(style as Record<string, unknown>)) {
    if (!STYLE_KEYS.has(k)) {
      text.fail(
        n,
        `style on <${name}> inside <Svg> does not take ${k}: it takes the SVG attributes CSS sets (fill, stroke, strokeWidth, opacity, fontSize, …).`,
      );
    }
    if (v !== undefined) props[k] = v;
  }
  return props;
}

function element(n: Node, font: DrawingFont, text: SvgTextFonts, inText: boolean): string {
  if (n.kind === 'text') {
    if (!inText) throw new Error('Internal error: the reconciler let text into an <Svg>.');
    return escapeXml(n.text);
  }
  const isText = n.type === HOST.Text || n.type === HOST.Tspan || n.type === HOST.TextPath;
  const tag = n.type === HOST.Text ? 'text' : SVG_TAGS[n.type];
  if (tag === undefined) throw new Error(`Internal error: the reconciler let <${n.type}> into an <Svg>.`);
  const props = ownProps(n, text);
  let inner: DrawingFont;
  try {
    inner = fontOf(font, props);
  } catch (e) {
    return text.fail(n, (e as Error).message);
  }
  const name = HOST_NAME[n.type];
  const layout = LAYOUT_ONLY.find((k) => n.props[k] !== undefined);
  if (layout !== undefined) {
    const where = layout === 'render' ? 'a <View> around the <Svg>' : 'the <Svg>';
    text.fail(n, `<${name}> inside <Svg> does not take \`${layout}\`: put it on ${where}.`);
  }
  let a: string;
  if (isText) {
    const face = text.face(inner, n);
    a = attrs(props, TEXT_SKIP);
    a += ` font-family="${escapeXml(quotedFamily(face.family[0] ?? ''))}" font-weight="${face.weight}"`;
    if (face.style === 'italic') a += ' font-style="italic"';
    if (face.stretch !== undefined && face.stretch !== 100) a += ` font-stretch="${face.stretch}%"`;
  } else {
    a = attrs(props, CHILD_SKIP);
  }
  const body = n.children.map((c) => element(c, inner, text, inText || isText)).join('');
  return body === '' ? `<${tag}${a}/>` : `<${tag}${a}>${body}</${tag}>`;
}

/** Whether `inst` holds SVG text. */
const hasText = (n: Node): boolean =>
  n.kind === 'element' && (n.type === HOST.Text || n.children.some(hasText));

/**
 * The SVG document of an `<Svg>` element, sized `width` × `height` points, with the paint of its
 * own style `own`. Its text is written with the faces `text` resolves, at the size around it
 * unless it sets one.
 */
export function svgMarkup(
  inst: Instance,
  width: number,
  height: number,
  text: SvgTextFonts,
  own: FlatStyle = {},
): string {
  const font = fontOf(text.font, inst.props);
  const body = inst.children.map((c) => element(c, font, text, false)).join('');
  let a = attrs(inst.props, BOX_PROPS);
  // The paint its classes or style set, which the drawing inherits.
  const paints: Record<string, unknown> = {};
  for (const k of ['fill', 'stroke', 'strokeWidth'] as const) {
    if (own[k] !== undefined) paints[k] = CLASS_SVG[k]?.(own[k]);
  }
  a += attrs(paints, CHILD_SKIP);
  // SVG's own `currentColor` is black: say the text colour around the drawing where it is another.
  if (inst.props.color === undefined && text.color.some((v, i) => v !== (i === 3 ? 1 : 0))) {
    a += ` color="${svgColor(text.color)}"`;
  }
  if (inst.children.some(hasText) && inst.props.fontSize === undefined) {
    a += ` font-size="${text.size}"`;
  }
  return svgDocument(width, height, body, inst.props.viewBox !== undefined, a);
}

/**
 * An SVG document of `width` × `height` points around `body` (`<Svg>` and `<Canvas>`). Its view
 * box is the same size unless `ownViewBox` says `attrs`, the root's other attributes, set one.
 */
export function svgDocument(
  width: number,
  height: number,
  body: string,
  ownViewBox = false,
  attrs = '',
): string {
  const viewBox = ownViewBox ? '' : ` viewBox="0 0 ${width} ${height}"`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"${viewBox}${attrs}>${body}</svg>`;
}

/** A `data:` URL for SVG markup. */
export function svgDataUrl(markup: string): string {
  return `data:image/svg+xml;base64,${base64(new TextEncoder().encode(markup))}`;
}

/** `bytes` in base64. */
export function base64(bytes: Uint8Array): string {
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(bin);
}
