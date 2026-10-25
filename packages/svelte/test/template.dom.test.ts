/** The templates mounted in a DOM, as the browser entry renders them, give what the server gives. */
import { FontRegistry, loadTailwind } from '@inkoxide/tree';
import { beforeAll, expect, it } from 'vitest';

import { renderTemplate } from '../src/dom.ts';
import Doc from './fixtures/Doc.svelte';

const fonts = new FontRegistry();
fonts.register({ id: 'Inter-Regular', family: 'Inter' });

beforeAll(async () => {
  await loadTailwind();
});

it('reads the components, their props, text, blocks and render snippets from the DOM', () => {
  const tree = renderTemplate({ component: Doc, props: { name: 'Ada' } }, { fonts, pageCount: 2 });
  const json = JSON.stringify(tree.document);
  expect(json).toContain('"text":"Hello Ada "');
  expect(json).toContain('"hyphenate":true');
  expect(json).toContain('"name":"email","kind":"text"');
  expect(json).toContain('2 / 2');
});
