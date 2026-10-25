// Angular's libraries are partially compiled; outside an Angular build they need the JIT compiler.
import '@angular/compiler';
import { renderTemplate as angularTree } from '@inkoxide/angular';
import { Document, Page, renderTemplate as reactTree, TemplateError, Text, View } from '@inkoxide/react';
import { renderTemplate as svelteTree } from '@inkoxide/svelte';
import * as V from '@inkoxide/vue';
import { createElement as r } from 'react';
import { describe, expect, it } from 'vitest';
import { defineComponent, h } from 'vue';

import * as Ng from '../angular/examples.ts';
import SvelteMisplaced from '../svelte/Misplaced.svelte';
import SvelteMisplacedInner from '../svelte/MisplacedInner.svelte';

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

  // The element is in a component of the template's own: each stack names it.
  const Inner = () => r(View);
  const MisplacedInner = () => r(Document, null, r(Page, null, r(Text, null, 'Hi ', r(Inner))));
  const reactInner = thrown(() => reactTree(r(MisplacedInner)));

  it.each([
    [
      'Vue',
      () => {
        const VInner = defineComponent({ name: 'Inner', setup: () => () => h(V.View) });
        const VMisplacedInner = defineComponent({
          name: 'MisplacedInner',
          setup: () => () =>
            h(V.Document, null, () => [h(V.Page, null, () => [h(V.Text, null, () => ['Hi ', h(VInner)])])]),
        });
        return V.renderTemplate(h(VMisplacedInner));
      },
    ],
    ['Svelte', () => svelteTree(SvelteMisplacedInner)],
    ['Angular', () => angularTree(Ng.MisplacedInner)],
  ])('%s names the component of the template it is in, as React does', (_, render) => {
    expect(thrown(render).componentStack).toBe(reactInner.componentStack);
  });
});
