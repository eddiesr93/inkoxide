import { parseFontFamily } from '@inkoxide/style';

import { type DrawingFont, escapeXml, quotedFamily } from '../canvas.ts';
import { HOST, type SvgPresentation, type SvgProps } from '../components/index.ts';
import type { Instance, Node } from '../reconciler/instance.ts';
import { weightOf } from './style.ts';

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
};
const BOX_PROPS: ReadonlySet<string> = new Set([...Object.keys(BOX_KEYS), 'children']);

const attrName = (k: string): string =>
  CAMEL_ATTRS.has(k) ? k : k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

function attrs(props: Record<string, unknown>, skip: ReadonlySet<string>): string {
  let out = '';
  for (const [k, v] of Object.entries(props)) {
    if (skip.has(k) || v === undefined || v === null || v === false) continue;
    const value = Array.isArray(v) ? v.join(' ') : String(v);
    out += ` ${attrName(k)}="${escapeXml(value)}"`;
  }
  return out;
}

const CHILD_SKIP = new Set(['children']);
/** The font props of SVG text, which it writes as the face they resolve to. */
const TEXT_SKIP = new Set(['children', 'fontFamily', 'fontWeight', 'fontStyle']);

/** What the text of a drawing needs from the tree builder. */
export interface SvgTextFonts {
  /** The text style around the drawing, which its text inherits. */
  font: DrawingFont;
  /** Its font size in points. */
  size: number;
  /** The registered face for `font` (marked as used): its family, weight and style. */
  face(font: DrawingFont, inst: Instance): DrawingFont;
  fail(inst: Instance, message: string): never;
}

/** `font` with the font props `props` set. */
function fontOf(font: DrawingFont, props: Record<string, unknown>): DrawingFont {
  const { fontFamily, fontWeight, fontStyle } = props;
  return {
    family: typeof fontFamily === 'string' ? parseFontFamily(fontFamily) : font.family,
    weight: fontWeight === undefined ? font.weight : weightOf(fontWeight as number | string),
    style: fontStyle === 'italic' ? 'italic' : fontStyle === 'normal' ? 'normal' : font.style,
  };
}

function element(n: Node, font: DrawingFont, text: SvgTextFonts, inText: boolean): string {
  if (n.kind === 'text') {
    if (!inText) throw new Error('Internal error: the reconciler let text into an <Svg>.');
    return escapeXml(n.text);
  }
  const isText = n.type === HOST.Text || n.type === HOST.Tspan;
  const tag = n.type === HOST.Text ? 'text' : SVG_TAGS[n.type];
  if (tag === undefined) throw new Error(`Internal error: the reconciler let <${n.type}> into an <Svg>.`);
  let inner: DrawingFont;
  try {
    inner = fontOf(font, n.props);
  } catch (e) {
    return text.fail(n, (e as Error).message);
  }
  let a: string;
  if (isText) {
    for (const key of ['style', 'className'] as const) {
      if (n.props[key] !== undefined) {
        text.fail(
          n,
          `${key} applies to <Text> outside <Svg>; SVG text takes SVG attributes (fill, fontSize, …).`,
        );
      }
    }
    const face = text.face(inner, n);
    a = attrs(n.props, TEXT_SKIP);
    a += ` font-family="${escapeXml(quotedFamily(face.family[0] ?? ''))}" font-weight="${face.weight}"`;
    if (face.style === 'italic') a += ' font-style="italic"';
  } else {
    a = attrs(n.props, CHILD_SKIP);
  }
  const body = n.children.map((c) => element(c, inner, text, inText || isText)).join('');
  return body === '' ? `<${tag}${a}/>` : `<${tag}${a}>${body}</${tag}>`;
}

/** Whether `inst` holds SVG text. */
const hasText = (n: Node): boolean =>
  n.kind === 'element' && (n.type === HOST.Text || n.children.some(hasText));

/**
 * The SVG document of an `<Svg>` element, sized `width` × `height` points. Its text is written
 * with the faces `text` resolves, at the size around it unless it sets one.
 */
export function svgMarkup(inst: Instance, width: number, height: number, text: SvgTextFonts): string {
  const font = fontOf(text.font, inst.props);
  const body = inst.children.map((c) => element(c, font, text, false)).join('');
  let a = attrs(inst.props, BOX_PROPS);
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
  const bytes = new TextEncoder().encode(markup);
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return `data:image/svg+xml;base64,${btoa(bin)}`;
}
