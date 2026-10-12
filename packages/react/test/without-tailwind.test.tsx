import { expect, it } from 'vitest';

import { Document, Page, renderTemplate, Text } from '../src/index.ts';
import { loadWith } from '../src/tailwind.ts';
import { registry, thrown } from './helpers.ts';

// Its own file: Tailwind is not loaded here, and the module state carries from test to test.
const page = (text: React.ReactNode) => (
  <Document>
    <Page size="A4">{text}</Page>
  </Document>
);

const missing = (name: string) =>
  Object.assign(new Error(`Cannot find package '${name}' imported from /app/index.js`), {
    code: 'ERR_MODULE_NOT_FOUND',
  });

it('styles with `style` alone need no Tailwind', () => {
  const r = renderTemplate(page(<Text style={{ fontSize: 18 }}>Hello</Text>), { fonts: registry() });
  expect(r.document.pages[0]?.children?.[0]).toMatchObject({ type: 'text', runs: [{ size: 18 }] });
  // The document's default families are Tailwind's `font-sans` all the same.
  expect((r.document.fonts ?? []).map((f) => f.family)).toContain('Inter');
});

it('className before loadTailwind says how to load it', () => {
  const e = thrown(() => renderTemplate(page(<Text className="text-lg">Hi</Text>), { fonts: registry() }));
  expect(e.kind).toBe('tailwind');
  expect(e.className).toBe('text-lg');
  expect(e.message).toContain('call `await loadTailwind()` before `renderTemplate`');
});

it('a missing import inside Tailwind is not mistaken for Tailwind missing', async () => {
  await expect(loadWith(() => Promise.reject(missing('tailwindcss')))).rejects.toThrow("'tailwindcss'");
});

it('className without Tailwind installed says to install it', async () => {
  expect(await loadWith(() => Promise.reject(missing('@inkoxide/tailwind')))).toBe(false);
  const e = thrown(() => renderTemplate(page(<Text className="text-lg">Hi</Text>), { fonts: registry() }));
  expect(e.message).toBe(
    'className="text-lg" on <Text> needs Tailwind: install it with `npm install @inkoxide/tailwind`.',
  );
});
