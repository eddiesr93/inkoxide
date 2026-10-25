// Svelte templates to document trees: the component renders once, then what it rendered is read
// into host nodes and walked into the schema tree.
import {
  type Container,
  finishTemplate,
  formatStack,
  type HostType,
  type Instance,
  type Node,
  type PageInfo,
  type RenderPage,
  templateError,
  type TemplateFunctions,
  templateFunctions,
} from '@inkoxide/tree';
import type { Component, Snippet } from 'svelte';

import { CAPTURE, type Capture } from './capture.ts';
import PageRoot from './components/PageRoot.svelte';

/**
 * What Svelte renders to a document: a component with its props (`{ component: Invoice, props:
 * { invoice } }`), or a component alone.
 */
// oxlint-disable-next-line typescript/no-explicit-any -- any component, whatever its props.
export type Renderable = Component<any> | { component: Component<any>; props?: Record<string, unknown> };

/**
 * How a render runs where it runs: `svelte/server` on a server, `mount` in a browser. It renders
 * `component` with `props` and `context`, and reads what it made with the render's table.
 */
export type Mounter = (
  // oxlint-disable-next-line typescript/no-explicit-any -- any component, whatever its props.
  component: Component<any>,
  props: Record<string, unknown>,
  context: Map<unknown, unknown>,
  table: Capture,
) => Node[];

const parts = (input: Renderable) => (typeof input === 'function' ? { component: input, props: {} } : input);

/** The name of the component an input renders, for errors. */
export const componentName = (input: Renderable): string => {
  const c = parts(input).component as { name?: string } | undefined;
  return (c?.name && c.name.replace(/^_?/, '')) || 'document';
};

/**
 * The components of a Svelte error's stack trace, innermost first: Svelte keeps no component stack,
 * but each component is the function of its file (`Invoice.svelte`). A component's snippets run
 * inside the components it gives them to, so a file's frames come before those of the components
 * around it too: each name keeps its outermost place. Empty when the code is bundled without file
 * names.
 */
export function svelteStack(error: unknown): string | undefined {
  const trace = error instanceof Error ? error.stack : undefined;
  if (!trace) return undefined;
  const files = [...trace.matchAll(/([^/\\\s()]+)\.svelte(?:\?[^:\s)]*)?:\d+/g)].map((m) => m[1] as string);
  const names = files.filter((name, i) => name !== 'PageRoot' && !files.includes(name, i + 1));
  return names.length ? formatStack(names) : undefined;
}

/** Renders `component` with Svelte, reading as many frames of what it throws as a stack needs. */
function rendered<T>(render: () => T): T {
  const limit = Error.stackTraceLimit;
  Error.stackTraceLimit = Math.max(limit, 200);
  try {
    return render();
  } catch (e) {
    throw templateError(e, svelteStack(e));
  } finally {
    Error.stackTraceLimit = limit;
  }
}

/** Renders and reads, then checks the nodes inside `parent` (the host type of a `render` snippet). */
function run(
  mounter: Mounter,
  // oxlint-disable-next-line typescript/no-explicit-any -- any component, whatever its props.
  component: Component<any>,
  props: Record<string, unknown>,
  contexts: Map<unknown, unknown>,
  parent?: HostType,
): Container {
  const table: Capture = { items: [] };
  const context = new Map(contexts);
  context.set(CAPTURE, table);
  return finishTemplate(
    rendered(() => mounter(component, props, context, table)),
    parent,
  );
}

/** The functions of the Svelte front end, for the way renders run where it runs. */
export function svelteTemplates(mounter: Mounter): TemplateFunctions<Renderable> {
  const renderPage: RenderPage = (inst: Instance, page: PageInfo): Node[] => {
    const { snippet, contexts } = inst.props.render as {
      snippet: Snippet<[PageInfo]>;
      contexts: Map<unknown, unknown>;
    };
    return run(mounter, PageRoot, { snippet, page }, contexts, inst.type).children;
  };

  return templateFunctions(
    (input: Renderable) => {
      const { component, props = {} } = parts(input);
      return run(mounter, component, props, new Map());
    },
    { renderPage },
  );
}
