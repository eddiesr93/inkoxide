import { escapeXml } from '../canvas.ts';
import { HOST, type SvgPresentation, type SvgProps } from '../components/index.ts';
import type { Instance, Node } from '../reconciler/instance.ts';

/** SVG element name of each primitive host type. */
export const SVG_TAGS: Record<string, string> = {
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

function element(n: Node, fail: (inst: Instance | undefined, msg: string) => never, owner: Instance): string {
  if (n.kind === 'text') {
    if (n.text.trim() === '') return '';
    return fail(owner, 'Text inside <Svg> is not supported; place a <Text> over the drawing instead.');
  }
  const tag = SVG_TAGS[n.type];
  if (!tag) return fail(n, `Only SVG primitives (Path, Rect, Circle, G, …) can be inside <Svg>.`);
  const body = n.children.map((c) => element(c, fail, n)).join('');
  const a = attrs(n.props, CHILD_SKIP);
  return body === '' ? `<${tag}${a}/>` : `<${tag}${a}>${body}</${tag}>`;
}

/** The SVG document of an `<Svg>` element, sized `width` × `height` points. */
export function svgMarkup(
  inst: Instance,
  width: number,
  height: number,
  fail: (inst: Instance | undefined, msg: string) => never,
): string {
  const body = inst.children.map((c) => element(c, fail, inst)).join('');
  const viewBox = inst.props.viewBox === undefined ? ` viewBox="0 0 ${width} ${height}"` : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"${viewBox}${attrs(inst.props, BOX_PROPS)}>${body}</svg>`;
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
