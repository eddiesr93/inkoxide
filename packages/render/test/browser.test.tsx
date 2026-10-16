/**
 * `@inkoxide/render/browser` renders what the main entry renders, to the same bytes: the same
 * pipeline, with the engine as WebAssembly. Here it runs on the calling thread (`worker: false`);
 * examples/browser runs it in a worker, in browsers. Each entry is loaded in a module graph of its
 * own, as in an application, since both keep process-wide state. Needs packages/wasm/inkoxide.wasm
 * (scripts/build-wasm.sh): without it the tests skip, except in CI.
 */
import { existsSync, readFileSync } from 'node:fs';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_FACES } from '../src/fonts.ts';
import { HYPHENATION_FILES } from '../src/hyphenation.ts';

const WASM = new URL('../../wasm/inkoxide.wasm', import.meta.url);
const built = existsSync(WASM);
if (!built && process.env.CI)
  throw new Error('packages/wasm/inkoxide.wasm is missing: run scripts/build-wasm.sh');

const PNG = readFileSync(new URL('../../../fixtures/images/logo.png', import.meta.url));
const OPTS = { creationDate: '2026-10-08T00:00:00Z' };

type Entry = typeof import('../src/browser.ts');

/** A page with text in several bundled faces, a class, an image and a table. */
function Doc({
  ink,
}: {
  ink: Pick<Entry, 'Document' | 'Page' | 'Text' | 'View' | 'Image' | 'Table' | 'Row' | 'Cell'>;
}) {
  const { Document, Page, Text, View, Image, Table, Row, Cell } = ink;
  return (
    <Document title="Parity">
      <Page size="A5" style={{ padding: 24 }}>
        <Text style={{ fontSize: 18, fontWeight: 'bold' }}>Invoice 2026-001</Text>
        <Text style={{ fontStyle: 'italic' }}>Hyphenation keeps extraordinarily long words in check.</Text>
        <View className="mt-4 flex-row gap-2">
          <Image src="logo" style={{ width: 32, height: 32 }} />
          <Text style={{ fontFamily: 'Roboto Mono' }}>INV-0001</Text>
        </View>
        <Table columns={['2fr', '1fr']}>
          <Row>
            <Cell>
              <Text>Item</Text>
            </Cell>
            <Cell>
              <Text style={{ textAlign: 'right' }}>12.50</Text>
            </Cell>
          </Row>
        </Table>
      </Page>
    </Document>
  );
}

async function load<T>(entry: string): Promise<T> {
  vi.resetModules();
  return (await import(/* @vite-ignore */ entry)) as T;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe.skipIf(!built)('@inkoxide/render/browser', () => {
  it('renders the bytes the main entry renders', async () => {
    const node = await load<typeof import('../src/index.ts')>('../src/index.ts');
    const expected = await node.renderToBuffer(<Doc ink={node} />, { ...OPTS, images: { logo: PNG } });

    const browser = await load<Entry>('../src/browser.ts');
    await browser.init({ worker: false });
    const pdf = await browser.renderToBuffer(<Doc ink={browser} />, { ...OPTS, images: { logo: PNG } });
    expect(Buffer.from(pdf).equals(Buffer.from(expected))).toBe(true);

    const blob = await browser.renderToBlob(<Doc ink={browser} />, { ...OPTS, images: { logo: PNG } });
    expect(blob.type).toBe('application/pdf');
    expect(Buffer.from(await blob.arrayBuffer()).equals(Buffer.from(expected))).toBe(true);
  });

  it('reads a bundled font only when a document uses it', async () => {
    const browser = await load<Entry>('../src/browser.ts');
    const status = await browser.init({ worker: false });
    expect(status.fonts).toEqual([]);
    const { Document, Page, Text } = browser;
    await browser.renderToBuffer(
      <Document>
        <Page>
          <Text>Hello</Text>
        </Page>
      </Document>,
    );
    expect(browser.engineStatus()?.fonts).toContain('Inter-Regular');
    expect(browser.engineStatus()?.fonts).not.toContain('RobotoMono-Regular');
    // Noto Sans, the fallback, only when a character needs it.
    expect(browser.engineStatus()?.fonts).not.toContain('NotoSans-Regular');
  });

  it('fetches the fallback font when a character needs it, for the bytes Node renders', async () => {
    // Inter has no U+01C4 (Ǆ); Noto Sans, the fallback, has.
    const node = await load<typeof import('../src/index.ts')>('../src/index.ts');
    const doc = (ink: Pick<Entry, 'Document' | 'Page' | 'Text'>) => (
      <ink.Document>
        <ink.Page>
          <ink.Text>{'Hello \u01C4'}</ink.Text>
        </ink.Page>
      </ink.Document>
    );
    const expected = await node.renderToBuffer(doc(node), OPTS);
    const browser = await load<Entry>('../src/browser.ts');
    await browser.init({ worker: false });
    const pdf = await browser.renderToBuffer(doc(browser), OPTS);
    expect(Buffer.from(pdf).equals(Buffer.from(expected))).toBe(true);
    // It drew with Noto Sans: the font is in the PDF.
    expect(Buffer.from(pdf).toString('latin1')).toMatch(/NotoSans-Regular/);
  });

  it('refuses what needs a file system', async () => {
    const browser = await load<Entry>('../src/browser.ts');
    await browser.init({ worker: false });
    const { Document, Page, Text } = browser;
    const doc = (
      <Document>
        <Page>
          <Text style={{ fontFamily: 'Path' }}>Hello</Text>
        </Page>
      </Document>
    );
    await expect(
      browser.renderToBuffer(doc, { localImages: { root: '.' } } as Parameters<Entry['renderToBuffer']>[1]),
    ).rejects.toMatchObject({ code: 'ENGINE_REJECTED', path: 'options.localImages' });
    browser.Font.register({ family: 'Path', src: 'fonts/Brand.ttf' });
    await expect(browser.renderToBuffer(doc)).rejects.toMatchObject({
      code: 'INVALID_FONT',
      message: expect.stringContaining('no file system'),
    });
  });

  it('stops a render when its signal aborts', async () => {
    const browser = await load<Entry>('../src/browser.ts');
    await browser.init({ worker: false });
    const { Document, Page, Text } = browser;
    const reason = new Error('stop');
    await expect(
      browser.renderToBuffer(
        <Document>
          <Page>
            <Text>Hello</Text>
          </Page>
        </Document>,
        { signal: AbortSignal.abort(reason) },
      ),
    ).rejects.toBe(reason);
  });
});

describe('the bundled assets of the browser host', () => {
  it('are the files of fonts.ts and hyphenation.ts', async () => {
    const { BUNDLED_ASSETS } = await load<typeof import('../src/browser-host.ts')>('../src/browser-host.ts');
    const files = [
      ...DEFAULT_FACES.map((f) => `fonts/${f.file}`),
      ...Object.values(HYPHENATION_FILES).map((f) => `hyphenation/${f}`),
    ];
    expect([...BUNDLED_ASSETS].sort()).toEqual(files.sort());
  });
});
