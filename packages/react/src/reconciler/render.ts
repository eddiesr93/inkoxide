import type { ReactNode } from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants.js';

import type { HostType } from '../components/index.ts';
import { TemplateError } from '../errors.ts';
import { hostConfig } from './host-config.ts';
import { type Container, hostName } from './instance.ts';

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
  return names.length ? names.map((n) => `\n    in ${n}`).join('') : undefined;
}

interface Captured {
  error: unknown;
  componentStack: string | undefined;
}

/**
 * Renders a React element into plain host instances, synchronously: one sync update, flushed,
 * then the container is read. Hooks and context work; Suspense does not, and effects
 * must not be relied on (their state updates never reach the document).
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
