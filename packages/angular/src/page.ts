import { inject } from '@angular/core';
import { type PageInfo, TemplateError } from '@inkoxide/tree';

import { INK_PAGE } from './renderer.ts';

/**
 * The page number and page count, in a component inside a `render` template. Elsewhere the page is
 * not known yet: content is laid out once for the whole document. Call it where `inject` works.
 */
export function injectPage(): PageInfo {
  const page = inject(INK_PAGE, { optional: true });
  if (!page) {
    throw new TemplateError(
      'injectPage() works only inside a render template (<ink-text [render]="page"> with <ng-template #page let-page>); use <ink-page-number> and <ink-total-pages> in ordinary text.',
      { kind: 'render' },
    );
  }
  return page;
}
