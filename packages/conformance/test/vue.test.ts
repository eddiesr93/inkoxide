import * as R from '@inkoxide/react';
import { FontRegistry, loadTailwind } from '@inkoxide/tree';
import * as V from '@inkoxide/vue';
import { createElement as r } from 'react';
import { beforeAll, expect, it } from 'vitest';
import { defineComponent, h, inject, provide } from 'vue';

const fonts = new FontRegistry();
for (const f of [
  { id: 'Inter-Regular', family: 'Inter' },
  { id: 'Inter-Bold', family: 'Inter', weight: 700 },
])
  fonts.register(f);

beforeAll(async () => {
  await loadTailwind();
});

it('builds the same document as React', () => {
  const react = R.renderTemplate(
    r(
      R.Document,
      { title: 'T' },
      r(
        R.Page,
        { size: 'A5', style: { padding: 20 } },
        r(
          R.View,
          { className: 'flex-row gap-2' },
          r(
            R.Text,
            { style: { fontWeight: 'bold' } },
            'Hello ',
            r(R.Link, { href: 'https://x.y' }, 'link'),
            ' end',
          ),
        ),
        r(
          R.Footer,
          null,
          r(R.Text, { render: ({ pageNumber, totalPages }: R.PageInfo) => `${pageNumber} / ${totalPages}` }),
        ),
      ),
    ),
    { fonts },
  );
  const vue = V.renderTemplate(
    h(V.Document, { title: 'T' }, () => [
      h(V.Page, { size: 'A5', style: { padding: 20 } }, () => [
        h(V.View, { class: 'flex-row gap-2' }, () => [
          h(V.Text, { style: { fontWeight: 'bold' } }, () => [
            'Hello ',
            h(V.Link, { href: 'https://x.y' }, () => 'link'),
            ' end',
          ]),
        ]),
        h(V.Footer, null, () => [
          h(V.Text, null, {
            render: ({ pageNumber, totalPages }: V.PageInfo) => [`${pageNumber} / ${totalPages}`],
          }),
        ]),
      ]),
    ]),
    { fonts },
  );
  expect(vue.document).toEqual(react.document);
  expect(vue.rebuild(3).document).toEqual(react.rebuild(3).document);
});

it('names the components above a misplaced element', () => {
  const Inner = defineComponent({ name: 'Inner', setup: () => () => h(V.View) });
  const Outer = defineComponent({
    name: 'Outer',
    setup: () => () =>
      h(V.Document, null, () => [h(V.Page, null, () => [h(V.Text, null, () => [h(Inner)])])]),
  });
  let err: unknown;
  try {
    V.renderTemplate(h(Outer), { fonts });
  } catch (e) {
    err = e;
  }
  expect(err).toBeInstanceOf(V.TemplateError);
  expect((err as V.TemplateError).message).toBe(
    '<View> cannot be inside <Text>. A Text contains only strings, numbers, <Text>, <Link>, <Image>, <Note>, <Attachment>, <PageNumber> and <TotalPages>.',
  );
  expect((err as V.TemplateError).componentStack).toBe(
    '\n    in View\n    in Inner\n    in Text\n    in Page\n    in Document\n    in Outer',
  );
});

it('gives usePage the page, and render slots what the components around them provide', () => {
  const Num = defineComponent({
    name: 'Num',
    setup: () => {
      const p = V.usePage();
      const who = inject('who');
      return () => `${who} ${p.pageNumber}`;
    },
  });
  const Doc = defineComponent({
    name: 'Doc',
    setup: () => {
      provide('who', 'p.');
      return () =>
        h(V.Document, null, () => [h(V.Page, null, () => [h(V.Text, null, { render: () => [h(Num)] })])]);
    },
  });
  const out = V.renderTemplate(h(Doc), { fonts, pageCount: 1 });
  expect(JSON.stringify(out.document)).toContain('p. 1');
});

it('reports what a component throws, with its stack', () => {
  const Bad = defineComponent({
    name: 'Bad',
    setup: () => () => {
      throw new Error('boom');
    },
  });
  let err: unknown;
  try {
    V.renderTemplate(
      h(V.Document, null, () => [h(V.Page, null, () => [h(Bad)])]),
      { fonts },
    );
  } catch (e) {
    err = e;
  }
  expect((err as V.TemplateError).message).toBe('The template threw: boom');
  expect((err as V.TemplateError).componentStack).toBe('\n    in Bad\n    in Page\n    in Document');
});
