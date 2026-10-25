import { type PageInfo, TemplateError } from '@inkoxide/tree';
import { inject } from 'vue';

import { PAGE } from './renderer.ts';

/**
 * The page number and page count, inside the content of a `render` slot (or a component it
 * renders). Elsewhere the page is not known yet: content is laid out once for the whole document.
 */
export function usePage(): PageInfo {
  const page = inject<PageInfo | undefined>(PAGE, undefined);
  if (!page) {
    throw new TemplateError(
      'usePage() works only inside a render slot (<Text><template #render="{ pageNumber }">…</template></Text>); use <PageNumber> and <TotalPages> in ordinary text.',
      { kind: 'render' },
    );
  }
  return page;
}
