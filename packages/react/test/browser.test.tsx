/**
 * `@inkoxide/render/browser` renders what the main entry renders, to the same bytes: the same
 * pipeline, with the engine as WebAssembly. Here it runs on the calling thread (`worker: false`);
 * examples/browser runs it in a worker, in browsers. Each entry is loaded in a module graph of its
 * own, as in an application, since both keep process-wide state. Needs packages/wasm/inkoxide.wasm
 * (scripts/build-wasm.sh): without it the tests skip, except in CI.
 */
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_FACES } from '../../render/src/fonts.ts';
import { HYPHENATION_FILES } from '../../render/src/hyphenation.ts';

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

    const stream = await browser.renderToStream(<Doc ink={browser} />, { ...OPTS, images: { logo: PNG } });
    const chunks: Uint8Array[] = [];
    for await (const chunk of stream) chunks.push(chunk);
    expect(chunks.length).toBe(Math.ceil(expected.byteLength / (64 * 1024)));
    expect(Buffer.concat(chunks).equals(Buffer.from(expected))).toBe(true);

    // A reader that transfers a chunk's buffer gets an error, not a PDF cut short: a PDF of a few
    // chunks.
    const { Document, Page, Text } = browser;
    const long = (
      <Document>
        {Array.from({ length: 40 }, (_, i) => (
          <Page key={i}>
            <Text>{'A page of the long document. '.repeat(40)}</Text>
          </Page>
        ))}
      </Document>
    );
    const reader = (await browser.renderToStream(long, { compress: false })).getReader();
    const { value } = await reader.read();
    structuredClone(value?.buffer, { transfer: [value?.buffer as ArrayBuffer] });
    const rest = (async () => {
      for (;;) if ((await reader.read()).done) return 'done';
    })();
    await expect(rest).rejects.toThrow('the PDF was transferred while it was read');
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
    // A file path is an error even where text is a URL on the page.
    vi.stubGlobal('location', { href: 'https://example.com/app/' });
    for (const [family, src] of [
      ['File URL', 'file:///fonts/Brand.ttf'],
      ['Windows path', 'C:\\fonts\\Brand.ttf'],
      ['Backslash', 'fonts\\Brand.ttf'],
    ] as const) {
      browser.Font.register({ family, src });
      await expect(
        browser.renderToBuffer(
          <Document>
            <Page>
              <Text style={{ fontFamily: family }}>Hello</Text>
            </Page>
          </Document>,
        ),
      ).rejects.toMatchObject({
        code: 'INVALID_FONT',
        message: expect.stringMatching(new RegExp(`${family}.*no file system`)),
      });
    }
    vi.unstubAllGlobals();
  });

  it('takes the bundled files from `assets`, and fails with INTERNAL on one it cannot fetch', async () => {
    // Only Inter Regular: neither Noto Sans nor the engine's German hyphenation patterns.
    const dir = mkdtempSync(join(tmpdir(), 'inkoxide-assets-'));
    mkdirSync(join(dir, 'fonts'));
    copyFileSync(
      new URL('../../render/assets/fonts/Inter-Regular.ttf', import.meta.url),
      join(dir, 'fonts/Inter-Regular.ttf'),
    );
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    const browser = await load<Entry>('../src/browser.ts');
    // A folder without the `/` at the end is the same folder.
    await browser.init({ worker: false, assets: pathToFileURL(dir) });
    const { Document, Page, Text } = browser;
    const page = (text: string, lang?: string) => (
      <Document lang={lang}>
        <Page>
          <Text hyphenate={lang !== undefined} style={{ width: 60 }}>
            {text}
          </Text>
        </Page>
      </Document>
    );
    expect((await browser.renderToBuffer(page('Hello'), OPTS)).length).toBeGreaterThan(0);
    const url = pathToFileURL(dir).href;
    await expect(browser.renderToBuffer(page('Hello \u01C4'), OPTS)).rejects.toMatchObject({
      code: 'INTERNAL',
      engineCode: 'NOT_LOADED',
      message: expect.stringContaining(`${url}/fonts/NotoSans-Regular.ttf`),
    });
    await expect(browser.renderToBuffer(page('Donaudampfschifffahrt', 'de'), OPTS)).rejects.toMatchObject({
      code: 'INTERNAL',
      engineCode: 'NOT_LOADED',
      message: expect.stringContaining(`${url}/hyphenation/de.bin`),
    });
  });

  it('falls back from hyphenation patterns it cannot read, and fails with INTERNAL on a font it cannot read', async () => {
    // What a server that answers a missing file with its page sends.
    const page = '<!doctype html><title>App</title>';
    const dir = mkdtempSync(join(tmpdir(), 'inkoxide-assets-'));
    mkdirSync(join(dir, 'fonts'));
    mkdirSync(join(dir, 'hyphenation'));
    writeFileSync(join(dir, 'fonts/Inter-Regular.ttf'), page);
    writeFileSync(join(dir, 'hyphenation/hyph-en-us.tex'), page);
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    const browser = await load<Entry>('../src/browser.ts');
    const status = await browser.init({ worker: false, assets: pathToFileURL(`${dir}/`) });
    expect(status.hyphenation).toEqual({ en: false });
    expect(status.warnings).toContainEqual(
      expect.stringMatching(
        /^en hyphenation patterns not readable: .*Text falls back to the engine's built-in patterns\.$/,
      ),
    );
    const { Document, Page, Text } = browser;
    const err = await browser
      .renderToBuffer(
        <Document>
          <Page>
            <Text>Hello</Text>
          </Page>
        </Document>,
      )
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(browser.RenderError);
    expect(err).toMatchObject({
      code: 'INTERNAL',
      status: 500,
      message: expect.stringContaining(
        `The bundled font Inter-Regular from ${pathToFileURL(dir).href}/fonts/Inter-Regular.ttf could not be loaded: `,
      ),
    });
  });

  it('keeps the options of the first init, and starts again after a start that failed', async () => {
    const browser = await load<Entry>('../src/browser.ts');
    // Not a WebAssembly module.
    const broken = browser.init({ worker: false, wasm: new Uint8Array([1, 2, 3, 4]) });
    // Before the engine has started: the options of the first call still count.
    const plain = browser.init();
    for (const call of [broken, plain]) {
      await expect(call).rejects.toMatchObject({
        code: 'INTERNAL',
        message: expect.stringMatching(/^The engine could not start: /),
      });
    }
    expect(browser.engineStatus()).toBeUndefined();
    const status = await browser.init({ worker: false });
    expect(status.version.schema).toBeGreaterThan(0);
    expect(browser.engineStatus()).toBe(status);
  });

  it('stops a render at timeoutMs, or when its signal aborts, while its images load', async () => {
    const browser = await load<Entry>('../src/browser.ts');
    await browser.init({ worker: false });
    const { Document, Page, Image } = browser;
    const slow = () => new Promise<Uint8Array>((ok) => setTimeout(() => ok(PNG), 2000));
    const doc = (
      <Document>
        <Page>
          <Image src={slow} style={{ width: 32, height: 32 }} />
        </Page>
      </Document>
    );
    const t0 = performance.now();
    const timedOut = await browser.renderToBuffer(doc, { timeoutMs: 50 }).catch((e: unknown) => e);
    expect(timedOut).toBeInstanceOf(DOMException);
    expect(timedOut).toMatchObject({ name: 'TimeoutError' });
    // 0 has passed already, however fast the render.
    const now = await browser
      .renderToBuffer(
        <Document>
          <Page />
        </Document>,
        { timeoutMs: 0 },
      )
      .catch((e: unknown) => e);
    expect(now).toBeInstanceOf(DOMException);
    expect(now).toMatchObject({ name: 'TimeoutError' });
    const reason = new Error('stop');
    const controller = new AbortController();
    setTimeout(() => controller.abort(reason), 50);
    await expect(browser.renderToBuffer(doc, { signal: controller.signal })).rejects.toBe(reason);
    expect(performance.now() - t0).toBeLessThan(1000);
  });

  it('checks timeoutMs, and renders the same after clearCaches', async () => {
    const browser = await load<Entry>('../src/browser.ts');
    await browser.init({ worker: false });
    await expect(browser.clearCaches()).resolves.toBeUndefined();
    const doc = <Doc ink={browser} />;
    const opts = { ...OPTS, images: { logo: PNG } };
    await expect(browser.renderToBuffer(doc, { ...opts, timeoutMs: -1 })).rejects.toMatchObject({
      code: 'ENGINE_REJECTED',
      path: 'options.timeoutMs',
    });
    const pdf = await browser.renderToBuffer(doc, { ...opts, timeoutMs: 60_000 });
    await browser.clearCaches();
    expect(Buffer.from(await browser.renderToBuffer(doc, opts)).equals(Buffer.from(pdf))).toBe(true);
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
    const { BUNDLED_ASSETS } = await load<typeof import('../../render/src/browser-host.ts')>(
      '../../render/src/browser-host.ts',
    );
    const files = [
      ...DEFAULT_FACES.map((f) => `fonts/${f.file}`),
      ...Object.values(HYPHENATION_FILES).map((f) => `hyphenation/${f}`),
    ];
    expect([...BUNDLED_ASSETS].sort()).toEqual(files.sort());
  });
});
