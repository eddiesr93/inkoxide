// Angular's libraries are partially compiled; outside an Angular build they need the JIT compiler.
import '@angular/compiler';
import { renderTemplate as angularTree } from '@inkoxide/angular';
import { Document, Page, renderTemplate as reactTree, TemplateError, Text, View } from '@inkoxide/react';
import { renderTemplate as svelteTree } from '@inkoxide/svelte';
import * as V from '@inkoxide/vue';
import { createElement as r } from 'react';
import { describe, expect, it } from 'vitest';
import { h } from 'vue';

import * as Ng from '../angular/examples.ts';
import SvelteMisplaced from '../svelte/Misplaced.svelte';

/** The TemplateError that `fn` throws. */
function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}

describe('a component in the wrong place', () => {
  const react = thrown(() => reactTree(r(Document, null, r(Page, null, r(Text, null, 'Hi ', r(View))))));

  it.each([
    [
      'Vue',
      () =>
        V.renderTemplate(
          h(V.Document, null, () => [h(V.Page, null, () => [h(V.Text, null, () => ['Hi ', h(V.View)])])]),
        ),
    ],
    ['Svelte', () => svelteTree(SvelteMisplaced)],
    ['Angular', () => angularTree(Ng.Misplaced)],
  ])('%s says what React says, with the components around it', (_, render) => {
    const e = thrown(render);
    expect(e.message).toBe(react.message);
    expect(e.kind).toBe(react.kind);
    expect(e.componentStack).toMatch(/^\n    in View\n    in /);
  });
});
