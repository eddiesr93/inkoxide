// React templates to document trees: React renders once, synchronously, then the host nodes are
// walked into the schema tree.
import {
  componentStack,
  type Container,
  type Instance,
  type PageInfo,
  type RenderPage,
  TemplateError,
  type TemplateFunctions,
  templateFunctions,
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
const renderPage: RenderPage = (inst: Instance, page: PageInfo): Container => {
  const fn = inst.props.render as RenderProp;
  try {
    return renderToInstances(
      createElement(PageContext.Provider, { value: page }, createElement(RenderCall, { fn, page })),
      inst.type,
    );
  } catch (e) {
    // The content is rendered in a root of its own, whose stack ends at the call of `fn`: the
    // components around the element carry it on.
    if (e instanceof TemplateError && e.componentStack?.endsWith(RENDER_CALL)) {
      e.componentStack = e.componentStack.slice(0, -RENDER_CALL.length) + componentStack(inst);
    }
    throw e;
  }
};

/** React's template functions: React renders an element once, synchronously, into host nodes. */
export const { renderTemplate, renderToTree }: TemplateFunctions<ReactNode> = templateFunctions(
  (element: ReactNode) => renderToInstances(element),
  renderPage,
);
