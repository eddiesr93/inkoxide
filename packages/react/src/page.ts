import { type PageInfo, TemplateError } from '@inkoxide/tree';
import { createContext, type ReactNode, useContext } from 'react';

import type { RenderProp } from './components/index.ts';

/** The page a `render` prop is rendering for. */
export const PageContext = createContext<PageInfo | undefined>(undefined);

/**
 * The page number and page count, inside the output of a `render` prop (or a component it
 * renders). Elsewhere the page is not known yet: content is laid out once for the whole document.
 */
export function usePage(): PageInfo {
  const page = useContext(PageContext);
  if (!page) {
    throw new TemplateError(
      'usePage() works only inside a render prop (<Text render={…}> or <View render={…}>); use <PageNumber> and <TotalPages> in ordinary text.',
      { kind: 'render' },
    );
  }
  return page;
}

/** Calls a render prop as a component, so it may use hooks. */
export function RenderCall({ fn, page }: { fn: RenderProp; page: PageInfo }): ReactNode {
  return fn(page);
}
