import type { Node as SchemaNode, Style } from '@inkoxide/schema';

/**
 * The props of an element in the flow (`Builder.flowProps`) that a per-page wrapper takes from its
 * variants. `debug` is apart: both are outlined.
 */
export const FLOW_KEYS = ['id', 'break', 'link', 'bookmark', 'pages'] as const;
export type FlowKey = (typeof FLOW_KEYS)[number];

/**
 * Style keys that place an element among its siblings (kept on a per-page wrapper): floats too,
 * which do not float inside the grid cell the variants are stacked in.
 */
const OUTER_KEYS = [
  'margin',
  'inset',
  'position',
  'flexGrow',
  'flexShrink',
  'flexBasis',
  'alignSelf',
  'justifySelf',
  'gridColumn',
  'gridRow',
  'width',
  'height',
  'minWidth',
  'minHeight',
  'maxWidth',
  'maxHeight',
  'aspectRatio',
  'zIndex',
  'float',
  'clear',
  'shapeOutside',
  'shapeMargin',
  'rotate',
  'transform',
  'transformOrigin',
] as const;

/**
 * The variants of an element with a `render` prop, each with the pages it is painted on, stacked
 * in one grid cell of a wrapper box that takes the element's place in the layout.
 */
export function stackVariants(variants: ReadonlyArray<{ node: SchemaNode; pages: number[] }>): SchemaNode {
  const wrapper: SchemaNode = { type: 'box', children: [] };
  const outer: Style = { display: 'grid' };
  for (const { node, pages } of variants) {
    const v = node as SchemaNode & Record<string, unknown>;
    for (const k of FLOW_KEYS) {
      if (v[k] !== undefined) (wrapper as unknown as Record<string, unknown>)[k] = v[k];
      delete v[k];
    }
    // Both outlined: the wrapper holds the margins, the variant the border and padding.
    if (v.debug === true) wrapper.debug = true;
    const inner: Record<string, unknown> = { ...(v.style as Record<string, unknown> | undefined) };
    for (const k of OUTER_KEYS) {
      if (inner[k] !== undefined) (outer as Record<string, unknown>)[k] = inner[k];
      delete inner[k];
    }
    inner.gridRow = { start: 1 };
    inner.gridColumn = { start: 1 };
    v.style = inner as Style;
    // An own `pages` filter on the element narrows every variant.
    v.pages = pages as [number, ...number[]];
    wrapper.children?.push(v);
  }
  wrapper.style = outer;
  return wrapper;
}
