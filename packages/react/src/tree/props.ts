// What the props of an element say besides its style: block-only and flow-only props, breaks,
// bookmarks, and the props every node takes.
import type { Bookmark, Break } from '@inkoxide/schema';

import type { BookmarkProp, BreakProp } from '../components/index.ts';
import type { Instance, Node } from '../reconciler/instance.ts';

/**
 * Props of a `Text` or `Link` that need a node of its own: an inline run inside a `Text` has
 * none. `debug` is left out: on a run it draws nothing, as the guide says.
 */
export const BLOCK_ONLY = [
  'id',
  'bookmark',
  'pages',
  'render',
  'dir',
  'orphans',
  'widows',
  'break',
  'wrap',
  'minPresenceAhead',
] as const;

/** Whether a block-only prop is set to something other than its default (`wrap`, `break={false}`). */
export function blockOnly(props: Record<string, unknown>, key: (typeof BLOCK_ONLY)[number]): boolean {
  const v = props[key];
  return v !== undefined && !(key === 'wrap' && v === true) && !(key === 'break' && v === false);
}

export const isEl = (n: Node | undefined, type: string): n is Instance =>
  n?.kind === 'element' && n.type === type;

export function toBreak(b: BreakProp | undefined): Break | undefined {
  if (b === undefined) return undefined;
  if (b === 'avoid') return { inside: 'avoid' };
  if (b === 'page-before') return { before: 'page' };
  if (b === 'page-after') return { after: 'page' };
  return b;
}

/** Copies the props every element's node takes, `id` and `debug`. */
export function commonProps(node: { id?: string; debug?: boolean }, inst: Instance): void {
  if (typeof inst.props.id === 'string') node.id = inst.props.id;
  if (inst.props.debug === true) node.debug = true;
}

/** Props of an element in the flow, which a `Page`, a band, a row or a cell does not take. */
export const FLOW_ONLY = [
  'pages',
  'href',
  'bookmark',
  'render',
  'break',
  'wrap',
  'minPresenceAhead',
  'fixed',
] as const;

/** The first flow-only prop set on `inst` to something other than its default. */
export function flowOnlyProp(
  inst: Instance,
  skip: readonly string[] = [],
): (typeof FLOW_ONLY)[number] | undefined {
  return FLOW_ONLY.find((k) => {
    if (skip.includes(k)) return false;
    if (k === 'fixed') return isFixed(inst);
    if (k === 'break' && inst.props.break === false) return false;
    if (k === 'wrap' && inst.props.wrap === true) return false;
    return inst.props[k] !== undefined;
  });
}

/** Whether the element has the `fixed` prop. */
export function isFixed(inst: Instance): boolean {
  return inst.props.fixed !== undefined && inst.props.fixed !== false;
}

/** Whether `b` sets a pagination control: a break before, after or inside, or `minPresenceAhead`. */
export function paginates(b: Break | undefined): boolean {
  if (b === undefined) return false;
  const set = (v: string | undefined) => v !== undefined && v !== 'auto';
  return set(b.before) || set(b.after) || set(b.inside) || b.minPresenceAhead !== undefined;
}

export function toBookmark(b: BookmarkProp | undefined): Bookmark | undefined {
  if (b === undefined) return undefined;
  return typeof b === 'string' ? { title: b } : b;
}
