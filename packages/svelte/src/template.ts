// Svelte templates to document trees: the component renders once, then what it rendered is read
// into host nodes and walked into the schema tree.
import {
  type Container,
  finishTemplate,
  type HostType,
  type Instance,
  type Node,
  type PageInfo,
  type RenderPage,
  type TemplateFunctions,
  templateFunctions,
} from '@inkoxide/tree';
import { renderWithTrace } from '@inkoxide/tree/internal';
import type { Component, Snippet } from 'svelte';

import PageRoot from './components/PageRoot.svelte';
import { svelteStack } from './stack.ts';
import { PROPS_TABLE, type PropsTable } from './table.ts';

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
  table: PropsTable,
) => Node[];

/** The component an input renders, and its props. */
const componentAndProps = (input: Renderable) =>
  typeof input === 'function' ? { component: input, props: {} } : input;

/** The name of the component an input renders, for errors. */
export const componentName = (input: Renderable): string => {
  const c = componentAndProps(input).component as { name?: string } | undefined;
  return (c?.name && c.name.replace(/^_?/, '')) || 'document';
};

/** Renders and reads, then checks the nodes inside `parent` (the host type of a `render` snippet). */
function renderAndRead(
  mounter: Mounter,
  // oxlint-disable-next-line typescript/no-explicit-any -- any component, whatever its props.
  component: Component<any>,
  props: Record<string, unknown>,
  contexts: Map<unknown, unknown>,
  parent?: HostType,
): Container {
  const table: PropsTable = { entries: [] };
  const context = new Map(contexts);
  context.set(PROPS_TABLE, table);
  return finishTemplate(
    renderWithTrace(() => mounter(component, props, context, table), svelteStack),
    parent,
  );
}

/** The functions of the Svelte front end, for the way renders run where it runs. */
export function svelteTemplates(mounter: Mounter): TemplateFunctions<Renderable> {
  const renderPage: RenderPage = (inst: Instance, page: PageInfo): Container => {
    const { snippet, contexts } = inst.props.render as {
      snippet: Snippet<[PageInfo]>;
      contexts: Map<unknown, unknown>;
    };
    return renderAndRead(mounter, PageRoot, { snippet, page }, contexts, inst.type);
  };

  return templateFunctions((input: Renderable) => {
    const { component, props = {} } = componentAndProps(input);
    return renderAndRead(mounter, component, props, new Map());
  }, renderPage);
}
