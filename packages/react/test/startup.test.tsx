/**
 * An engine that cannot start fails every render with an `INTERNAL` RenderError. Its own file:
 * `init` runs once per module graph, and Vitest gives each test file a fresh one.
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import {
  countPages,
  Document,
  init,
  Page,
  RenderError,
  renderToBuffer,
  renderToFile,
  renderToStream,
  Text,
} from '../src/index.ts';

const Doc = () => (
  <Document>
    <Page>
      <Text>Hello</Text>
    </Page>
  </Document>
);

const before = process.env.INKOXIDE_ASSETS_DIR;
let empty: string;

beforeAll(() => {
  empty = mkdtempSync(join(tmpdir(), 'inkoxide-'));
  process.env.INKOXIDE_ASSETS_DIR = empty;
});

afterAll(() => {
  rmSync(empty, { recursive: true, force: true });
  if (before === undefined) delete process.env.INKOXIDE_ASSETS_DIR;
  else process.env.INKOXIDE_ASSETS_DIR = before;
});

describe('engine start-up', () => {
  it('fails init and every render call with an INTERNAL RenderError', async () => {
    const calls: Promise<unknown>[] = [
      init(),
      renderToBuffer(<Doc />),
      renderToStream(<Doc />),
      renderToFile(<Doc />, join(empty, 'out.pdf')),
    ];
    for (const call of calls) {
      const err = await call.catch((e: unknown) => e);
      expect(err).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({ code: 'INTERNAL', status: 500 });
      expect((err as RenderError).message).toBe(
        `The engine could not start: No fonts found in ${join(empty, 'fonts')}. Set INKOXIDE_ASSETS_DIR to a folder with the package's fonts/ and hyphenation/ folders (its assets/).`,
      );
    }
  });

  it('starts on a later call once the fonts are there', async () => {
    delete process.env.INKOXIDE_ASSETS_DIR;
    await expect(init()).resolves.toMatchObject({ fonts: expect.arrayContaining(['Inter-Regular']) });
    expect(countPages(await renderToBuffer(<Doc />))).toBe(1);
  });
});
