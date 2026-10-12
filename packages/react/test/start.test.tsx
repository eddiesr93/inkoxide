import { isTailwindReady } from '@inkoxide/tailwind';
import { expect, it } from 'vitest';

import { Document, Page, renderToTree, Text } from '../src/index.ts';
import { registry } from './helpers.ts';

// Its own file: nothing has loaded Tailwind here before renderToTree.
it('renderToTree loads Tailwind when it is installed', async () => {
  expect(isTailwindReady()).toBe(false);
  const doc = await renderToTree(
    <Document>
      <Page size="A4">
        <Text className="text-2xl font-bold">Hello</Text>
      </Page>
    </Document>,
    { fonts: registry() },
  );
  expect(isTailwindReady()).toBe(true);
  expect(doc.pages[0]?.children?.[0]).toMatchObject({ type: 'text', runs: [{ size: 18 }] });
});
