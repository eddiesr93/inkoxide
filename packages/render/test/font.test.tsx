/**
 * `Font.register`: families from paths, URLs and bytes, loaded before the next render.
 * `Font.registerHyphenationCallback`: word breaks for every render.
 */
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { fonts, renderTemplate } from '@inkoxide/react';
import type { ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { assetsDir } from '../src/engine.ts';
import { countPages, Document, Font, Page, RenderError, renderToBuffer, Text } from '../src/index.ts';

const file = (name: string) => join(assetsDir(), 'fonts', name);

const Doc = ({
  family,
  weight = 400,
  italic = false,
}: {
  family: string;
  weight?: number;
  italic?: boolean;
}) => (
  <Document>
    <Page>
      <Text style={{ fontFamily: family, fontWeight: weight, fontStyle: italic ? 'italic' : 'normal' }}>
        Hello
      </Text>
    </Page>
  </Document>
);

/** The fonts the built tree declares, and whether the builder had to fall back. */
const treeFonts = (el: React.ReactElement) => {
  const t = renderTemplate(el);
  return { fonts: t.document.fonts, warnings: t.warnings };
};

let server: ReturnType<typeof createServer>;
let base: string;

beforeAll(async () => {
  server = createServer((req, res) => {
    if (req.url === '/Inter-Regular.ttf') res.end(readFileSync(file('Inter-Regular.ttf')));
    else if (req.url === '/moved') res.writeHead(302, { location: '/Inter-Regular.ttf' }).end();
    // Accepts the request and never answers.
    else if (req.url === '/stall') return;
    else res.writeHead(404).end();
  });
  await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
  base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(() => {
  server.closeAllConnections();
  return new Promise<void>((ok) => server.close(() => ok()));
});

describe('Font.register', () => {
  it('adds a family from a path and resolves its weight by name', async () => {
    Font.register({ family: 'Brand', src: file('Inter-Bold.ttf'), fontWeight: 'bold' });
    const pdf = await renderToBuffer(<Doc family="Brand" weight={700} />);
    expect(pdf.byteLength).toBeGreaterThan(0);
    const t = treeFonts(<Doc family="Brand" weight={700} />);
    expect(t.fonts).toContainEqual({ id: 'Brand-700', family: 'Brand', weight: 700 });
    expect(t.warnings).toEqual([]);
  });

  it('registers several faces from bytes and a file URL', async () => {
    Font.register({
      family: 'Brand Two',
      fonts: [
        { src: readFileSync(file('Inter-Regular.ttf')) },
        { src: pathToFileURL(file('Inter-Italic.ttf')), fontStyle: 'italic' },
      ],
    });
    await renderToBuffer(<Doc family="Brand Two" italic />);
    expect(fonts.has('Brand-Two-400')).toBe(true);
    expect(treeFonts(<Doc family="Brand Two" italic />).fonts).toContainEqual({
      id: 'Brand-Two-400-italic',
      family: 'Brand Two',
      style: 'italic',
    });
  });

  it('reads a file: URL as a string and as a URL', async () => {
    const url = pathToFileURL(file('Inter-Bold.ttf'));
    Font.register({ family: 'File String', src: url.href });
    Font.register({ family: 'File Object', src: url });
    await renderToBuffer(<Doc family="File String" />);
    expect(fonts.has('File-String-400')).toBe(true);
    expect(fonts.has('File-Object-400')).toBe(true);
  });

  it('fetches a font from a URL, following redirects', async () => {
    Font.register({ family: 'Remote', src: `${base}/Inter-Regular.ttf` });
    Font.register({ family: 'Remote Moved', src: `${base}/moved` });
    await renderToBuffer(<Doc family="Remote" />);
    expect(fonts.has('Remote-400')).toBe(true);
    expect(fonts.has('Remote-Moved-400')).toBe(true);
  });

  it('gives up on a font URL that does not answer, without holding up later renders', async () => {
    // The fetch has a 30-second limit; this test lets it pass in 200 ms.
    const timeout = AbortSignal.timeout.bind(AbortSignal);
    const spy = vi.spyOn(AbortSignal, 'timeout').mockImplementation(() => timeout(200));
    try {
      Font.register({ family: 'Stalled', src: `${base}/stall` });
      const [loading, other] = await Promise.allSettled([
        renderToBuffer(<Doc family="Stalled" />),
        renderToBuffer(<Doc family="Brand" />),
      ]);
      expect(loading).toMatchObject({ status: 'rejected', reason: { code: 'INVALID_FONT' } });
      expect((loading as PromiseRejectedResult).reason.message).toBe(
        `Font "Stalled" 400 (${base}/stall) could not be loaded: timed out after 30000 ms`,
      );
      expect(other.status).toBe('fulfilled');
    } finally {
      spy.mockRestore();
    }
    await expect(renderToBuffer(<Doc family="Brand" />)).resolves.toBeInstanceOf(Uint8Array);
  });

  it('registers WOFF and WOFF2 fonts', async () => {
    Font.register({ family: 'Web Mono', src: file('woff/RobotoMono-Regular.woff2') });
    Font.register({ family: 'Web Mono One', src: file('woff/RobotoMono-Regular.woff') });
    for (const family of ['Web Mono', 'Web Mono One']) {
      const pdf = await renderToBuffer(<Doc family={family} />);
      expect(pdf.byteLength).toBeGreaterThan(0);
      expect(treeFonts(<Doc family={family} />).warnings).toEqual([]);
    }
  });

  it('fails the render with the family and source when a font cannot be loaded, once', async () => {
    Font.register({ family: 'Missing', src: '/nowhere/Missing.ttf' });
    const err = await renderToBuffer(<Doc family="Brand" />).catch((e) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect(err.code).toBe('INVALID_FONT');
    expect(err.message).toContain('Font "Missing" 400 (/nowhere/Missing.ttf)');
    // The failed face is dropped: the next render works and can register it again.
    await expect(renderToBuffer(<Doc family="Brand" />)).resolves.toBeInstanceOf(Uint8Array);
    Font.register({ family: 'Missing', src: file('Inter-Regular.ttf') });
    await expect(renderToBuffer(<Doc family="Missing" />)).resolves.toBeInstanceOf(Uint8Array);
  });

  it('loads the other faces of a family when one cannot be loaded', async () => {
    Font.register({
      family: 'Half',
      fonts: [{ src: '/nowhere/Half.ttf' }, { src: file('Inter-Bold.ttf'), fontWeight: 'bold' }],
    });
    await expect(renderToBuffer(<Doc family="Brand" />)).rejects.toMatchObject({ code: 'INVALID_FONT' });
    expect(fonts.has('Half-400')).toBe(false);
    expect(fonts.has('Half-700')).toBe(true);
    await expect(renderToBuffer(<Doc family="Half" weight={700} />)).resolves.toBeInstanceOf(Uint8Array);
    expect(treeFonts(<Doc family="Half" weight={700} />).warnings).toEqual([]);
  });

  it('rejects a file that is not a font, and a 404', async () => {
    Font.register({ family: 'Garbage', src: new Uint8Array([1, 2, 3, 4]) });
    await expect(renderToBuffer(<Doc family="Brand" />)).rejects.toMatchObject({ code: 'INVALID_FONT' });
    Font.register({ family: 'Gone', src: `${base}/Gone.ttf` });
    await expect(renderToBuffer(<Doc family="Brand" />)).rejects.toThrow('HTTP 404');
  });

  it('ignores the same face registered twice, and refuses a different source for it', () => {
    const src = file('Inter-Bold.ttf');
    expect(() => Font.register({ family: 'Brand', src, fontWeight: 700 })).not.toThrow();
    expect(() => Font.register({ family: 'Brand', src: file('Inter-Regular.ttf'), fontWeight: 700 })).toThrow(
      'already registered',
    );
    expect(() => Font.register({ family: 'Brand', src, fontWeight: 'heavyish' as never })).toThrow(
      'not 1–1000 or a CSS weight name',
    );
  });
});

describe('Font.registerHyphenationCallback', () => {
  afterEach(() => Font.registerHyphenationCallback(null));

  // Two of these words do not fit on a 400pt line, and five lines fit on the page.
  const Words = () => (
    <Document>
      <Page size={{ width: 400, height: 110 }} style={{ fontSize: 10, lineHeight: 2 }}>
        <Text>{'Pneumonoultramicroscopicsilicovolcanoconiosis '.repeat(6)}</Text>
      </Page>
    </Document>
  );
  const threes = (word: string) => word.match(/.{1,3}/gu) ?? [word];

  it('breaks words where the callback says, in later renders', async () => {
    expect(countPages(await renderToBuffer(<Words />))).toBe(2);
    Font.registerHyphenationCallback(threes);
    expect(renderTemplate(<Words />).document.pages[0].children?.[0]).toMatchObject({
      runs: [{ text: expect.stringMatching(/^Pne\u00ADumo\u00ADnou/u) }],
    });
    expect(countPages(await renderToBuffer(<Words />))).toBe(1);
    Font.registerHyphenationCallback((word) => [word]);
    expect(countPages(await renderToBuffer(<Words />))).toBe(2);
  });

  const Para = ({ hyphenate, children }: { hyphenate?: boolean; children: ReactNode }) => (
    <Document lang="en-US">
      <Page size={{ width: 400, height: 110 }} style={{ fontSize: 10, lineHeight: 2 }}>
        <Text hyphenate={hyphenate}>{children}</Text>
      </Page>
    </Document>
  );
  const words = 'Pneumonoultramicroscopicsilicovolcanoconiosis '.repeat(6);

  it('gives way to hyphenate={false}', async () => {
    Font.registerHyphenationCallback(threes);
    expect(countPages(await renderToBuffer(<Para>{words}</Para>))).toBe(1);
    expect(countPages(await renderToBuffer(<Para hyphenate={false}>{words}</Para>))).toBe(2);
  });

  it('keeps the patterns off a run with its own callback, and brings them back with null', async () => {
    expect(countPages(await renderToBuffer(<Para hyphenate>{words}</Para>))).toBe(1);
    const kept = (
      <Para hyphenate>
        <Text hyphenationCallback={(word) => [word]}>{words}</Text>
      </Para>
    );
    expect(countPages(await renderToBuffer(kept))).toBe(2);
    Font.registerHyphenationCallback((word) => [word]);
    expect(countPages(await renderToBuffer(<Para hyphenate>{words}</Para>))).toBe(2);
    const patterns = (
      <Para hyphenate>
        <Text hyphenationCallback={null}>{words}</Text>
      </Para>
    );
    expect(countPages(await renderToBuffer(patterns))).toBe(1);
  });

  it('is removed with null and rejects anything but a function', async () => {
    Font.registerHyphenationCallback(threes);
    Font.registerHyphenationCallback(null);
    expect(countPages(await renderToBuffer(<Words />))).toBe(2);
    expect(() => Font.registerHyphenationCallback('en' as never)).toThrow('needs a function');
  });
});
