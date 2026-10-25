import type { FrontEnd, TemplateLike as AnyTemplate } from '@inkoxide/render';
import { type ComponentType, createElement, type ReactElement } from 'react';
import type { z } from 'zod';

import { renderTemplate } from './template.ts';

/** A template component: it takes its data in the prop `data`. */
// oxlint-disable-next-line typescript/no-explicit-any -- whatever data the template takes.
export type TemplateComponent = ComponentType<{ data: any }>;

/**
 * A template for `generate`: a component that takes `{ data }`, the zod schema of that data and
 * the static images it refers to by id.
 */
export type TemplateLike<S extends z.ZodType = z.ZodType> = AnyTemplate<
  S,
  ComponentType<{ data: z.output<S> }>
>;

/** React as a front end of `@inkoxide/render`: elements in, document trees out. */
export const react: FrontEnd<ReactElement, TemplateComponent> = {
  renderTemplate,
  withData: (Template, data) => createElement(Template, { data }),
  componentName(element) {
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
