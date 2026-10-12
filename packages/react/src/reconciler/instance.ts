import { HOST_NAME, type HostType } from '../components/index.ts';

/** A host element: plain data, no DOM and no engine types. */
export interface Instance {
  kind: 'element';
  type: HostType;
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
  parent?: HostType;
}

/** True for the host types of this package's components; false for `div` and other elements. */
export function isHostType(type: string): type is HostType {
  return Object.hasOwn(HOST_NAME, type);
}

/** The component name of a host type (`View`), or the type of an element that is not one (`div`). */
export function hostName(type: string): string {
  return isHostType(type) ? HOST_NAME[type] : type;
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
  if (f.tag === HOST_COMPONENT && typeof f.type === 'string') return hostName(f.type);
  if (f.tag === undefined || !COMPONENT_TAGS.has(f.tag)) return undefined;
  let t = f.type as
    | { displayName?: string; name?: string; render?: { name?: string }; type?: unknown }
    | undefined;
  if (f.tag === 11 && t?.render) t = t.render as typeof t;
  if ((f.tag === 14 || f.tag === 15) && t && typeof t === 'object' && 'type' in t) t = t.type as typeof t;
  return t?.displayName ?? t?.name ?? undefined;
}

/** The components above a host instance, innermost first: its own name, then its parents'. */
export function componentNames(node: { fiber: unknown } | undefined): string[] {
  let f = node?.fiber as FiberLike | null | undefined;
  const names: string[] = [];
  while (f) {
    const n = nameOf(f);
    if (n && names[names.length - 1] !== n) names.push(n);
    f = f.return;
  }
  return names;
}

/**
 * The component stack of a host instance, innermost first, in React's own format
 * (`\n    in Text\n    in Field\n    in Template`).
 */
export function componentStack(node: { fiber: unknown } | undefined): string {
  return formatStack(componentNames(node));
}

/** Component names, innermost first, as a stack in React's format. */
export function formatStack(names: readonly string[]): string {
  return names.map((n) => `\n    in ${n}`).join('');
}

/** The innermost component of a stack in React's format: `Text` for `\n    in Text\n    in Field`. */
export function innermostComponent(stack: string | undefined): string | undefined {
  return stack?.split('\n')[1]?.trim().replace(/^in /, '');
}
