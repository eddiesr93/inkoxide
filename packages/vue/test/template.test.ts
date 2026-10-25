import { FontRegistry, loadTailwind } from '@inkoxide/tree';
import { beforeAll, describe, expect, it } from 'vitest';
import { defineAsyncComponent, defineComponent, h, Suspense } from 'vue';

import {
  Document,
  Image,
  Page,
  type PageInfo,
  renderTemplate,
  renderToBuffer,
  TemplateError,
  Text,
  TextField,
  View,
} from '../src/index.ts';

const fonts = new FontRegistry();
fonts.register({ id: 'Inter-Regular', family: 'Inter' });

beforeAll(async () => {
  await loadTailwind();
});

/** A document of one page with `content`. */
const page = (...content: unknown[]) => h(Document, null, () => [h(Page, null, () => content)]);

function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}

describe('props', () => {
  it('camelizes attribute names, reads class as className, and gives bare boolean attributes true', () => {
    const tree = renderTemplate(
      page(h(Text, { class: 'text-lg', 'hyphenation-penalty': 900, hyphenate: '' }, () => 'Hi')),
      { fonts },
    );
    expect(tree.document.pages[0]?.children?.[0]).toMatchObject({
      type: 'text',
      style: { lineHeight: 1.555556, hyphenate: true, hyphenationPenalty: 900 },
    });
  });

  it('leaves out listeners', () => {
    const tree = renderTemplate(page(h(View, { onClick: () => {} })), { fonts });
    expect(JSON.stringify(tree.document)).not.toContain('onClick');
  });

  it('sets the kind of the field components', () => {
    const tree = renderTemplate(page(h(TextField, { name: 'email' })), { fonts });
    expect(tree.document.pages[0]?.children?.[0]).toMatchObject({
      type: 'field',
      field: { kind: 'text', name: 'email' },
    });
  });

  it('drops what v-if leaves where nothing renders', () => {
    const tree = renderTemplate(
      page(
        null,
        h(Text, null, () => ['a', null, 'b']),
      ),
      { fonts },
    );
    expect(tree.document.pages[0]?.children).toHaveLength(1);
  });
});

describe('errors', () => {
  it('refuses an element in the wrong place, with the components above it', () => {
    const e = thrown(() => renderTemplate(page(h(Text, null, () => [h(View)])), { fonts }));
    expect(e.message).toMatch(/^<View> cannot be inside <Text>/);
    expect(e.componentStack).toBe('\n    in View\n    in Text\n    in Page\n    in Document');
  });

  it('refuses a string outside a text container', () => {
    const e = thrown(() =>
      renderTemplate(
        h(Document, null, () => ['loose']),
        { fonts },
      ),
    );
    expect(e.message).toBe('Text "loose" cannot be directly inside <Document>; put it in a <Page>.');
  });

  it('refuses an async component', () => {
    const Later = defineAsyncComponent(() => new Promise<never>(() => {}));
    const e = thrown(() => renderTemplate(page(h(Later)), { fonts }));
    expect(e.message).toMatch(/^An async component was not loaded\./);
  });

  it('refuses an async setup()', () => {
    const Slow = defineComponent({
      name: 'Slow',
      async setup() {
        await new Promise(() => {});
        return () => null;
      },
    });
    const e = thrown(() => renderTemplate(page(h(Suspense, null, { default: () => h(Slow) })), { fonts }));
    expect(e.message).toMatch(/Suspense|async setup/);
  });

  it('says where usePage() works', async () => {
    const { usePage } = await import('../src/index.ts');
    const Bad = defineComponent({
      name: 'Bad',
      setup: () => {
        usePage();
        return () => null;
      },
    });
    const e = thrown(() => renderTemplate(page(h(Bad)), { fonts }));
    expect(e.message).toMatch(/^usePage\(\) works only inside a render slot/);
    expect(e.componentStack).toBe('\n    in Bad\n    in Page\n    in Document');
  });
});

describe('render slot', () => {
  it('renders its content for each page', () => {
    const doc = (n: number) =>
      renderTemplate(
        page(
          h(Text, null, { render: ({ pageNumber, totalPages }: PageInfo) => `${pageNumber}/${totalPages}` }),
        ),
        { fonts, pageCount: n },
      );
    expect(doc(1).dynamic).toBe(true);
    expect(JSON.stringify(doc(2).document)).toContain('2/2');
  });
});

describe('renderToBuffer', () => {
  it('renders a VNode, and a component', async () => {
    const Doc = defineComponent({ name: 'Doc', setup: () => () => page(h(Text, null, () => 'Hello')) });
    const a = await renderToBuffer(h(Doc), { creationDate: '2026-10-25T00:00:00Z' });
    const b = await renderToBuffer(Doc, { creationDate: '2026-10-25T00:00:00Z' });
    expect(new TextDecoder().decode(a.subarray(0, 5))).toBe('%PDF-');
    expect(b).toEqual(a);
  });

  it('names the component in a RenderError', async () => {
    const Broken = defineComponent({ name: 'Broken', setup: () => () => page(h(Image)) });
    await expect(renderToBuffer(h(Broken))).rejects.toMatchObject({ code: expect.any(String) });
  });
});
