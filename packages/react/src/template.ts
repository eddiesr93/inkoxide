// React templates to document trees: React renders once, synchronously, then the host nodes are
// walked into the schema tree.
import type { Document } from '@inkoxide/schema';
import {
  buildDocument,
  componentStack,
  type Instance,
  loadTailwind,
  type Node,
  type PageInfo,
  type RenderPage,
  TemplateError,
  type TreeOptions,
  type TreeResult,
} from '@inkoxide/tree';
import { createElement, type ReactNode } from 'react';

import type { RenderProp } from './components/index.ts';
import { PageContext, RenderCall } from './page.ts';
import { renderToInstances } from './reconciler/render.ts';

/** The end of the component stack of an error inside a `render` prop's own root. */
const RENDER_CALL = `\n    in ${RenderCall.name}`;

/**
 * A `render` prop's content for one page, rendered by React in a root of its own inside the
 * element's host type, with the page in `PageContext` for `usePage()`.
 */
const renderPage: RenderPage = (inst: Instance, page: PageInfo): Node[] => {
  const fn = inst.props.render as RenderProp;
  try {
    return renderToInstances(
      createElement(PageContext.Provider, { value: page }, createElement(RenderCall, { fn, page })),
      inst.type,
    ).children;
  } catch (e) {
    // The content is rendered in a root of its own, whose stack ends at the call of `fn`: the
    // components around the element carry it on.
    if (e instanceof TemplateError && e.componentStack?.endsWith(RENDER_CALL)) {
      e.componentStack = e.componentStack.slice(0, -RENDER_CALL.length) + componentStack(inst);
    }
    throw e;
  }
};

/**
 * Renders a template element to an Inkoxide document. Synchronous: React renders once, then the
 * host nodes are walked into the schema tree. Throws TemplateError with a component stack.
 */
export function renderTemplate(element: ReactNode, opts: TreeOptions = {}): TreeResult {
  const t0 = performance.now();
  const container = renderToInstances(element);
  return buildDocument(container, { renderPage }, opts, performance.now() - t0);
}

/**
 * The document tree of a template element (the simple API). Loads Tailwind first when it is
 * installed (`loadTailwind`). Building is synchronous; a template error is a rejected promise,
 * not a throw.
 */
export async function renderToTree(element: ReactNode, opts: TreeOptions = {}): Promise<Document> {
  await loadTailwind();
  return renderTemplate(element, opts).document;
}
