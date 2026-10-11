import { HOST_NAME } from '../components/index.ts';

/** A host element: plain data, no DOM and no engine types. */
export interface Instance {
  kind: 'element';
  type: string;
  props: Record<string, unknown>;
  children: Node[];
  /** The fiber, kept only to compute a component stack when something goes wrong. */
  fiber: unknown;
}

export interface TextInstance {
  kind: 'text';
  text: string;
  fiber: unknown;
}

export type Node = Instance | TextInstance;

export interface Container {
  children: Node[];
  /** Host type the content is rendered into, for nested roots (`render` props). */
  parent?: string;
}

interface FiberLike {
  tag?: number;
  type?: unknown;
  elementType?: unknown;
  return?: FiberLike | null;
}

// React work tags (stable across 18/19): function, class, forward-ref, memo, simple memo, host.
const COMPONENT_TAGS = new Set([0, 1, 11, 14, 15]);
const HOST_COMPONENT = 5;

function nameOf(f: FiberLike): string | undefined {
  if (f.tag === HOST_COMPONENT && typeof f.type === 'string') return HOST_NAME[f.type] ?? f.type;
  if (f.tag === undefined || !COMPONENT_TAGS.has(f.tag)) return undefined;
  let t = f.type as
    | { displayName?: string; name?: string; render?: { name?: string }; type?: unknown }
    | undefined;
  if (f.tag === 11 && t?.render) t = t.render as typeof t;
  if ((f.tag === 14 || f.tag === 15) && t && typeof t === 'object' && 'type' in t) t = t.type as typeof t;
  return t?.displayName ?? t?.name ?? undefined;
}

/**
 * The component stack of a host instance, innermost first, in React's own format
 * (`\n    in Text\n    in Field\n    in Template`).
 */
export function componentStack(node: { fiber: unknown } | undefined): string {
  let f = node?.fiber as FiberLike | null | undefined;
  const names: string[] = [];
  while (f) {
    const n = nameOf(f);
    if (n && names[names.length - 1] !== n) names.push(n);
    f = f.return;
  }
  return names.map((n) => `\n    in ${n}`).join('');
}
