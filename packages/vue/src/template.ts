// Vue templates to document trees: Vue renders once, synchronously, then the host nodes are
// walked into the schema tree.
import {
  type Instance,
  type Node,
  type PageInfo,
  type RenderPage,
  type TemplateFunctions,
  templateFunctions,
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

/** Vue's template functions: Vue renders a VNode or a component once, synchronously, into host nodes. */
export const { renderTemplate, renderToTree }: TemplateFunctions<Renderable> = templateFunctions(
  (input: Renderable) => mount(rootOf(input)),
  { renderPage },
);
