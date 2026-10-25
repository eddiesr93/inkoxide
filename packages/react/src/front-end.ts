import type { FrontEnd } from '@inkoxide/render';
import { createElement, type ComponentType, type ReactElement } from 'react';

import { renderTemplate } from './template.ts';

/** React as a front end of `@inkoxide/render`: elements in, document trees out. */
export const react: FrontEnd<ReactElement, ComponentType<{ data: never }>> = {
  tree: renderTemplate,
  withData: (Template, data) => createElement(Template as ComponentType<{ data: unknown }>, { data }),
  name(element) {
    // JavaScript callers can pass anything.
    const t = (element as { type?: unknown } | null | undefined)?.type as
      | string
      | { displayName?: string; name?: string }
      | null
      | undefined;
    if (typeof t === 'string') return t;
    return (t && (t.displayName ?? t.name)) || 'document';
  },
};
