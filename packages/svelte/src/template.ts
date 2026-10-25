// Svelte templates to document trees: the component renders once, then what it rendered is read
// into host nodes and walked into the schema tree.
import type { Document } from '@inkoxide/schema';
import {
  buildDocument,
  checkTree,
  type Container,
  type HostType,
  type Instance,
  loadTailwind,
  type Node,
  type PageInfo,
  type RenderPage,
  TemplateError,
  type TreeOptions,
  type TreeResult,
  trimTemplateText,
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

function templateError(error: unknown): TemplateError {
  if (error instanceof TemplateError) return error;
  const message = error instanceof Error ? error.message : String(error);
  return new TemplateError(`The template threw: ${message}`, { kind: 'render', cause: error });
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
  let nodes: Node[];
  try {
    nodes = mounter(component, props, context, table);
  } catch (e) {
    throw templateError(e);
  }
  const container: Container = parent === undefined ? { children: [] } : { children: [], parent };
  container.children = trimTemplateText(nodes, parent);
  checkTree(container);
  return container;
}

/** The functions of the Svelte front end, for the way renders run where it runs. */
export function svelteTemplates(mounter: Mounter) {
  const renderPage: RenderPage = (inst: Instance, page: PageInfo): Node[] => {
    const { snippet, contexts } = inst.props.render as {
      snippet: Snippet<[PageInfo]>;
      contexts: Map<unknown, unknown>;
    };
    return run(mounter, PageRoot, { snippet, page }, contexts, inst.type).children;
  };

  /**
   * Renders a Svelte template to an Inkoxide document. Synchronous: the component renders once,
   * then what it made is walked into the schema tree. Throws TemplateError.
   */
  function renderTemplate(input: Renderable, opts: TreeOptions = {}): TreeResult {
    const t0 = performance.now();
    const { component, props = {} } = parts(input);
    const container = run(mounter, component, props, new Map());
    return buildDocument(container, { renderPage }, opts, performance.now() - t0);
  }

  /**
   * The document tree of a Svelte template (the simple API). Loads Tailwind first when it is
   * installed. A template error is a rejected promise, not a throw.
   */
  async function renderToTree(input: Renderable, opts: TreeOptions = {}): Promise<Document> {
    await loadTailwind();
    return renderTemplate(input, opts).document;
  }

  return { renderTemplate, renderToTree };
}
