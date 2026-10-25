import { type PageInfo, TemplateError } from '@inkoxide/tree';
import { getContext } from 'svelte';

import { PAGE } from './table.ts';

/**
 * The page number and page count, in a component inside a `render` snippet. Elsewhere the page is
 * not known yet: content is laid out once for the whole document. Call it while the component is
 * created, as `getContext`.
 */
export function usePage(): PageInfo {
  const page = getContext<PageInfo | undefined>(PAGE);
  if (!page) {
    throw new TemplateError(
      'usePage() works only inside a render snippet ({#snippet render(page)} in a <Text> or <View>); use <PageNumber> and <TotalPages> in ordinary text.',
      { kind: 'render' },
    );
  }
  return page;
}
