import { FontRegistry, loadTailwind } from '@inkoxide/tree';
import { beforeAll, describe, expect, it } from 'vitest';

import { renderTemplate, renderToBuffer } from '../src/index.ts';
import Doc from './fixtures/Doc.svelte';

const fonts = new FontRegistry();
fonts.register({ id: 'Inter-Regular', family: 'Inter' });

beforeAll(async () => {
  await loadTailwind();
});

describe('templates', () => {
  it('reads the components, their props, text, blocks and render snippets', () => {
    const tree = renderTemplate({ component: Doc, props: { name: 'Ada' } }, { fonts, pageCount: 2 });
    const json = JSON.stringify(tree.document);
    expect(tree.document.pages[0]?.margin).toEqual({ top: 20, right: 20, bottom: 20, left: 20 });
    expect(json).toContain('"text":"Hello Ada "');
    expect(json).toContain('"hyphenate":true');
    expect(json).toContain('"name":"email","kind":"text"');
    expect(json).toContain('2 / 2');
  });

  it('renders to a PDF', async () => {
    const pdf = await renderToBuffer(
      { component: Doc, props: { name: 'Ada' } },
      { creationDate: '2026-10-25T00:00:00Z' },
    );
    expect(new TextDecoder().decode(pdf.subarray(0, 5))).toBe('%PDF-');
  });
});
