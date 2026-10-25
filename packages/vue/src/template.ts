// Vue templates to document trees: Vue renders once, synchronously, then the host nodes are
// walked into the schema tree.
import type { Document } from '@inkoxide/schema';
import {
  buildDocument,
  type Instance,
  loadTailwind,
  type Node,
  type PageInfo,
  type RenderPage,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/tree';
import { type Component, h, isVNode, type VNode } from 'vue';

import { mount, PageRoot } from './renderer.ts';

/** What Vue renders to a document: a VNode (`h(Invoice, { invoice })`), or a component. */
export type Renderable = VNode | Component;

/** The root component of a render. */
const rootOf = (input: Renderable): object => (isVNode(input) ? { name: '', render: () => input } : input);

/** A `render` slot's content for one page, rendered in a root of its own inside the element. */
const renderPage: RenderPage = (inst: Instance, page: PageInfo): Node[] =>
  mount(
    {
      name: '',
      render: () => h(PageRoot, { host: inst, page, fn: inst.props.render as (page: PageInfo) => unknown }),
    },
    inst.type,
  ).children;

/**
 * Renders a Vue template to an Inkoxide document. Synchronous: Vue renders once, then the host
 * nodes are walked into the schema tree. Throws TemplateError with a component stack.
 */
export function renderTemplate(input: Renderable, opts: TreeOptions = {}): TreeResult {
  const t0 = performance.now();
  const container = mount(rootOf(input));
  return buildDocument(container, { renderPage }, opts, performance.now() - t0);
}

/**
 * The document tree of a Vue template (the simple API). Loads Tailwind first when it is
 * installed. A template error is a rejected promise, not a throw.
 */
export async function renderToTree(input: Renderable, opts: TreeOptions = {}): Promise<Document> {
  await loadTailwind();
  return renderTemplate(input, opts).document;
}
