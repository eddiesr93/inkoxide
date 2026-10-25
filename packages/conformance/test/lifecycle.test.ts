/**
 * What every front end does around a render: a component deeper in a `render` prop's content
 * reads the page (`usePage`, `injectPage`), and what the template rendered, the document and the
 * roots of its pages, is released once the PDF is made, or at once when the render fails.
 */
import '@angular/compiler';
import { createHash } from 'node:crypto';

import { renderToBuffer as angularPdf, renderTemplate as angularTree } from '@inkoxide/angular';
import * as R from '@inkoxide/react';
import { renderToBuffer as sveltePdf, renderTemplate as svelteTree } from '@inkoxide/svelte';
import * as V from '@inkoxide/vue';
import { createElement as r, useLayoutEffect } from 'react';
import { beforeEach, describe, expect, it } from 'vitest';
import { defineComponent, h, onMounted, onUnmounted } from 'vue';

import * as Ng from '../angular/examples.ts';
import { live } from '../live.ts';
import SvelteMisplaced from '../svelte/Misplaced.svelte';
import SveltePaged from '../svelte/Paged.svelte';

const sha = (pdf: Uint8Array) => createHash('sha256').update(pdf).digest('hex');

/** Counts the React component in `live` while it is mounted. */
function useCounted(): void {
  useLayoutEffect(() => {
    live.created++;
    return () => {
      live.released++;
    };
  }, []);
}

function ReactCounted() {
  useCounted();
  return r(R.Text, null, 'Body');
}

function ReactPageLabel() {
  const page = R.usePage();
  useCounted();
  return `Page ${page.pageNumber} of ${page.totalPages}`;
}

const reactPaged = r(
  R.Document,
  null,
  r(
    R.Page,
    { size: 'A6' },
    r(R.Text, { render: () => r(ReactPageLabel) }),
    r(ReactCounted),
    r(R.PageBreak),
    r(R.Text, null, 'More'),
  ),
);

/** Counts the Vue component in `live` while it is mounted. */
function counted(): void {
  onMounted(() => live.created++);
  onUnmounted(() => live.released++);
}

const VueCounted = defineComponent({
  name: 'Counted',
  setup() {
    counted();
    return () => h(V.Text, null, () => ['Body']);
  },
});

const VuePageLabel = defineComponent({
  name: 'PageLabel',
  setup() {
    const page = V.usePage();
    counted();
    return () => `Page ${page.pageNumber} of ${page.totalPages}`;
  },
});

const vuePaged = h(V.Document, null, () => [
  h(V.Page, { size: 'A6' }, () => [
    h(V.Text, null, { render: () => [h(VuePageLabel)] }),
    h(VueCounted),
    h(V.PageBreak),
    h(V.Text, null, () => ['More']),
  ]),
]);

beforeEach(() => {
  live.created = 0;
  live.released = 0;
});

describe('a render', () => {
  let react = '';

  beforeEach(async () => {
    react ||= sha(await R.renderToBuffer(reactPaged));
    live.created = 0;
    live.released = 0;
  });

  it.each([
    ['React', () => R.renderToBuffer(reactPaged)],
    ['Vue', () => V.renderToBuffer(vuePaged)],
    ['Svelte', () => sveltePdf(SveltePaged)],
    ['Angular', () => angularPdf(Ng.Paged)],
  ])(
    'in %s gives the page to a component in a render prop, and releases what it rendered',
    async (_, pdf) => {
      expect(sha(await pdf())).toBe(react);
      // The document's component, and a label in each page's root of each pass.
      expect(live.created).toBeGreaterThan(2);
      expect(live.released).toBe(live.created);
    },
  );

  it.each([
    ['React', () => R.renderTemplate(reactPaged)],
    ['Vue', () => V.renderTemplate(vuePaged)],
    ['Svelte', () => svelteTree(SveltePaged)],
    ['Angular', () => angularTree(Ng.Paged)],
  ])('in %s keeps what it rendered until the tree is disposed', (_, build) => {
    const tree = build();
    expect(live.created).toBeGreaterThan(0);
    expect(tree.rebuild(2).pageCount).toBe(2);
    tree.dispose();
    expect(live.released).toBe(live.created);
  });
});

describe('a render that fails', () => {
  const ReactMisplaced = () => {
    useCounted();
    return r(R.Document, null, r(R.Page, null, r(R.Text, null, 'Hi ', r(R.View))));
  };
  const VueMisplaced = defineComponent({
    name: 'Misplaced',
    setup() {
      counted();
      return () =>
        h(V.Document, null, () => [h(V.Page, null, () => [h(V.Text, null, () => ['Hi ', h(V.View)])])]);
    },
  });

  it.each([
    ['React', () => R.renderTemplate(r(ReactMisplaced))],
    ['Vue', () => V.renderTemplate(h(VueMisplaced))],
    ['Svelte', () => svelteTree(SvelteMisplaced)],
    ['Angular', () => angularTree(Ng.Misplaced)],
  ])('in %s is released at once', (_, build) => {
    expect(build).toThrow(R.TemplateError);
    expect(live.released).toBe(live.created);
  });
});
