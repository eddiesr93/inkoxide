import type { ReactNode } from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants.js';

import type { HostType } from '../components/index.ts';
import { TemplateError } from '../errors.ts';
import { RenderCall } from '../page.ts';
import { hostConfig } from './host-config.ts';
import { type Container, componentNames, formatStack, hostName, nameOf } from './instance.ts';

// oxlint-disable-next-line typescript/no-explicit-any -- the reconciler's generic host-config typing is not worth replicating.
const reconciler = Reconciler(hostConfig as any);

/**
 * React 19 prints component stacks V8-style (`\n    at Field (/path/Template.tsx:12:3)`).
 * Normalise them to the `\n    in Field` form used everywhere else, with host types shown as
 * component names (`ink-view` → `View`).
 */
export function normalizeStack(stack: string | null | undefined): string | undefined {
  if (!stack) return undefined;
  const names: string[] = [];
  for (const line of stack.split('\n')) {
    const m = /^\s*(?:at|in)\s+([^\s(]+)/.exec(line);
    if (!m?.[1]) continue;
    const name = hostName(m[1]);
    if (names[names.length - 1] !== name) names.push(name);
  }
  return names.length ? formatStack(names) : undefined;
}

interface Captured {
  error: unknown;
  componentStack: string | undefined;
}

interface Fiber {
  tag: number;
  type: unknown;
  elementType: unknown;
  flags: number;
  memoizedState: unknown;
  child: Fiber | null;
  sibling: Fiber | null;
}

interface FiberRoot {
  current: Fiber & { alternate: Fiber | null; memoizedState: { element?: unknown } | null };
}

// React internals (stable across 18/19): the work tags of `<Suspense>` and of a `lazy` component
// not loaded yet, and the flag of a fiber whose render did not complete.
const SUSPENSE_COMPONENT = 13;
const LAZY_COMPONENT = 16;
const INCOMPLETE = 32768;

/** The first fiber of a tree, depth first, for which `test` holds and for none of its descendants. */
function innermost(f: Fiber | null, test: (f: Fiber) => boolean): Fiber | undefined {
  for (; f; f = f.sibling) {
    const inner = innermost(f.child, test);
    if (inner) return inner;
    if (test(f)) return f;
  }
  return undefined;
}

/**
 * What suspended, as the message names it (`Chart`, `A lazy component`, `The render prop of
 * <Text>`), and the name the component stack lacks, if any: a lazy component's is not in it.
 */
function suspender(f: Fiber, parent: HostType | undefined): { what: string; missing?: string } {
  if (f.tag === LAZY_COMPONENT) {
    const name = (f.elementType as { displayName?: unknown } | null)?.displayName;
    return typeof name === 'string' && name ? { what: name, missing: name } : { what: 'A lazy component' };
  }
  if (f.type === RenderCall && parent !== undefined) {
    return { what: `The render prop of <${hostName(parent)}>` };
  }
  return { what: nameOf(f) ?? 'A component' };
}

/**
 * The error for a render that suspended (`use()` of a promise, a lazy component): with no
 * `<Suspense>` boundary nothing commits, and the component that suspended is the innermost
 * unfinished fiber of the render; with one, the boundary commits its fallback, and React keeps no
 * trace of the component that suspended. `parent` is the host type a `render` prop renders into.
 */
function suspension(
  root: FiberRoot,
  element: ReactNode,
  parent: HostType | undefined,
): TemplateError | undefined {
  const advice =
    'Suspense, use() with a promise and lazy components are not supported: a template renders once, synchronously. Load the data first and pass it in as props.';
  if (root.current.memoizedState?.element !== element) {
    const at = innermost(root.current.alternate?.child ?? null, (f) => (f.flags & INCOMPLETE) !== 0);
    if (!at) return undefined;
    const { what, missing } = suspender(at, parent);
    const names = componentNames({ fiber: at });
    if (missing !== undefined) names.unshift(missing);
    return new TemplateError(`${what} suspended. ${advice}`, {
      kind: 'render',
      componentStack: formatStack(names),
    });
  }
  const boundary = innermost(
    root.current.child,
    (f) => f.tag === SUSPENSE_COMPONENT && f.memoizedState !== null,
  );
  if (!boundary) return undefined;
  return new TemplateError(
    `A component inside <Suspense> suspended, so the document would show its fallback. ${advice}`,
    { kind: 'render', componentStack: formatStack(['Suspense', ...componentNames({ fiber: boundary })]) },
  );
}

/**
 * Renders a React element into plain host instances, synchronously: one sync update, flushed,
 * then the container is read. Hooks and context work; a render that suspends is an error, and
 * effects must not be relied on (their state updates never reach the document).
 */
export function renderToInstances(element: ReactNode, parent?: HostType): Container {
  const container: Container = parent === undefined ? { children: [] } : { children: [], parent };
  let captured: Captured | undefined;
  const onUncaughtError = (error: unknown, info: { componentStack?: string | null }) => {
    captured ??= { error, componentStack: normalizeStack(info.componentStack) };
  };
  const noop = () => {};
  const root = reconciler.createContainer(
    container,
    ConcurrentRoot,
    null,
    false,
    null,
    '',
    onUncaughtError,
    noop,
    noop,
    noop,
    null,
  );
  reconciler.updateContainerSync(element, root, null, null);
  reconciler.flushSyncWork();
  const suspended = captured ? undefined : suspension(root as unknown as FiberRoot, element, parent);
  if (suspended) {
    // Unmounted, so React does not render the template again when the promise settles.
    reconciler.updateContainerSync(null, root, null, null);
    reconciler.flushSyncWork();
    throw suspended;
  }
  if (captured) {
    const { error, componentStack } = captured;
    if (error instanceof TemplateError) {
      error.componentStack ??= componentStack;
      throw error;
    }
    const message = error instanceof Error ? error.message : String(error);
    throw new TemplateError(`The template threw: ${message}`, {
      kind: 'render',
      componentStack,
      cause: error,
    });
  }
  return container;
}
