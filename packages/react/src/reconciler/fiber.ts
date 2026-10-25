import { hostName, type Trace } from '@inkoxide/tree';

interface FiberLike {
  tag?: number;
  type?: unknown;
  elementType?: unknown;
  return?: FiberLike | null;
}

// React work tags (stable across 18/19): function, class, forward-ref, memo, simple memo, host.
const COMPONENT_TAGS = new Set([0, 1, 11, 14, 15]);
const HOST_COMPONENT = 5;

/** The component name of a fiber: a host type's (`View`), a component's, else undefined. */
export function nameOf(f: FiberLike): string | undefined {
  if (f.tag === HOST_COMPONENT && typeof f.type === 'string') return hostName(f.type);
  if (f.tag === undefined || !COMPONENT_TAGS.has(f.tag)) return undefined;
  let t = f.type as
    | { displayName?: string; name?: string; render?: { name?: string }; type?: unknown }
    | undefined;
  if (f.tag === 11 && t?.render) t = t.render as typeof t;
  if ((f.tag === 14 || f.tag === 15) && t && typeof t === 'object' && 'type' in t) t = t.type as typeof t;
  return t?.displayName ?? t?.name ?? undefined;
}

/** The components above a fiber, innermost first: its own name, then its parents'. */
export const fiberNames: Trace = (fiber) => {
  let f = fiber as FiberLike | null | undefined;
  const names: string[] = [];
  while (f) {
    const n = nameOf(f);
    if (n && names[names.length - 1] !== n) names.push(n);
    f = f.return;
  }
  return names;
};
