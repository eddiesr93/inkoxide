import {
  checkElement,
  checkText,
  childContext,
  type Container,
  type HostContext,
  type Instance,
  type Node,
  rootContext,
  type TextInstance,
} from '@inkoxide/tree';
import { createContext } from 'react';
import { DefaultEventPriority, NoEventPriority } from 'react-reconciler/constants.js';

import { VERSION } from '../version.ts';
import { fiberNames } from './fiber.ts';

let currentPriority: number = NoEventPriority;

const append = (parent: Instance | Container, child: Node): void => {
  parent.children.push(child);
};
const insert = (parent: Instance | Container, child: Node, before: Node): void => {
  const at = parent.children.indexOf(child);
  if (at >= 0) parent.children.splice(at, 1);
  const i = parent.children.indexOf(before);
  parent.children.splice(i < 0 ? parent.children.length : i, 0, child);
};
const remove = (parent: Instance | Container, child: Node): void => {
  const i = parent.children.indexOf(child);
  if (i >= 0) parent.children.splice(i, 1);
};

/**
 * The host config: a mutation-mode renderer whose instances are plain objects. Templates render
 * once and synchronously, so the update paths exist only for completeness.
 */
export const hostConfig = {
  rendererPackageName: '@inkoxide/react',
  rendererVersion: VERSION,
  extraDevToolsConfig: null,
  isPrimaryRenderer: false,
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  supportsMicrotasks: true,
  scheduleMicrotask: queueMicrotask,
  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: -1,
  NotPendingTransition: null,
  HostTransitionContext: createContext(null),

  // A nested root (a `render` prop's output) starts inside its owner's host type.
  getRootHostContext: (root: Container): HostContext => rootContext(root.parent),
  getChildHostContext: childContext,
  getPublicInstance: (instance: Instance): Instance => instance,
  prepareForCommit: (): null => null,
  resetAfterCommit: (): void => {},
  preparePortalMount: (): void => {},

  createInstance(
    type: string,
    props: Record<string, unknown>,
    _root: Container,
    ctx: HostContext,
    fiber: unknown,
  ): Instance {
    checkElement(ctx, type);
    return { kind: 'element', type, props, children: [], origin: fiber, trace: fiberNames };
  },
  createTextInstance(text: string, _root: Container, ctx: HostContext, fiber: unknown): TextInstance {
    checkText(ctx, text);
    return { kind: 'text', text, origin: fiber, trace: fiberNames };
  },
  appendInitialChild: append,
  appendChild: append,
  appendChildToContainer: append,
  insertBefore: insert,
  insertInContainerBefore: insert,
  removeChild: remove,
  removeChildFromContainer: remove,
  clearContainer(container: Container): void {
    container.children.length = 0;
  },
  finalizeInitialChildren: (): boolean => false,
  shouldSetTextContent: (): boolean => false,
  commitMount: (): void => {},
  commitUpdate(instance: Instance, _type: string, _old: unknown, next: Record<string, unknown>): void {
    instance.props = next;
  },
  commitTextUpdate(instance: TextInstance, _old: string, next: string): void {
    instance.text = next;
  },
  resetTextContent: (): void => {},
  hideInstance: (): void => {},
  unhideInstance: (): void => {},
  hideTextInstance: (): void => {},
  unhideTextInstance: (): void => {},
  detachDeletedInstance: (): void => {},

  setCurrentUpdatePriority(p: number): void {
    currentPriority = p;
  },
  getCurrentUpdatePriority: (): number => currentPriority,
  resolveUpdatePriority: (): number =>
    currentPriority !== NoEventPriority ? currentPriority : DefaultEventPriority,
  resolveEventType: (): null => null,
  resolveEventTimeStamp: (): number => -1.1,
  trackSchedulerEvent: (): void => {},
  shouldAttemptEagerTransition: (): boolean => false,
  requestPostPaintCallback: (): void => {},
  getInstanceFromNode: (): null => null,
  getInstanceFromScope: (): null => null,
  beforeActiveInstanceBlur: (): void => {},
  afterActiveInstanceBlur: (): void => {},
  prepareScopeUpdate: (): void => {},
  resetFormInstance: (): void => {},

  maySuspendCommit: (): boolean => false,
  maySuspendCommitOnUpdate: (): boolean => false,
  maySuspendCommitInSyncRender: (): boolean => false,
  preloadInstance: (): boolean => true,
  startSuspendingCommit: (): void => {},
  suspendInstance: (): void => {},
  waitForCommitToBeReady: (): null => null,
  getSuspendedCommitReason: (): null => null,
  // React calls it when it commits work it retries after a suspension; a template that suspends
  // fails before then, but the retry can still run.
  suspendOnActiveViewTransition: (): void => {},
};
