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

import { countPages, Document, Font, Page, RenderError, renderToBuffer, Svg, Text } from '../src/index.ts';
import { assetsDir } from '../src/node-host.ts';

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
  it('draws a variable font at each weight of its range, and at its fontVariationSettings', async () => {
    const varFont = new URL('../../../fixtures/fonts/VarTest-glyf.ttf', import.meta.url);
    Font.register({ family: 'Var Test', src: varFont, fontWeight: [100, 900] });
    const el = (
      <Document>
        <Page>
          <Text style={{ fontFamily: 'Var Test', fontWeight: 300 }}>HOTEL</Text>
          <Text style={{ fontFamily: 'Var Test', fontWeight: 'bold' }}>HOTEL</Text>
          <Text style={{ fontFamily: 'Var Test', fontVariationSettings: "'wdth' 60, 'wght' 650" }}>
            HOTEL
          </Text>
        </Page>
      </Document>
    );
    const pdf = await renderToBuffer(el);
    const text = Buffer.from(pdf).toString('latin1');
    for (const name of ['VarTest_300wght', 'VarTest_700wght', 'VarTest_60wdth_650wght']) {
      expect(text).toContain(name);
    }
    const t = treeFonts(el);
    expect(t.fonts?.filter((f) => f.source === 'Var-Test-100-900')).toEqual([
      {
        id: 'Var-Test-100-900@wdth=60,wght=650',
        family: 'Var Test',
        weight: 650,
        stretch: 60,
        source: 'Var-Test-100-900',
        variations: { wdth: 60, wght: 650 },
      },
      {
        id: 'Var-Test-100-900@wght=300',
        family: 'Var Test',
        weight: 300,
        source: 'Var-Test-100-900',
        variations: { wght: 300 },
      },
      {
        id: 'Var-Test-100-900@wght=700',
        family: 'Var Test',
        weight: 700,
        source: 'Var-Test-100-900',
        variations: { wght: 700 },
      },
    ]);
    expect(() => Font.register({ family: 'Var Bad', src: varFont, fontWeight: [900, 100] })).toThrow(
      'the fontWeight range [900, 100] goes from the lower weight',
    );
  });

  it('draws a variable font at each width of its range, in text and in SVG text', async () => {
    const varFont = new URL('../../../fixtures/fonts/VarTest-glyf.ttf', import.meta.url);
    Font.register({ family: 'Var Wide', src: varFont, fontWeight: [100, 900], fontStretch: [50, 100] });
    const el = (
      <Document>
        <Page>
          <Text style={{ fontFamily: 'Var Wide', fontWeight: 700, fontStretch: 'condensed' }}>HOTEL</Text>
          <Svg width={200} height={40}>
            <Text x={0} y={30} fontFamily="Var Wide" fontStretch="62.5%">
              HOTEL
            </Text>
          </Svg>
        </Page>
      </Document>
    );
    const pdf = await renderToBuffer(el);
    const text = Buffer.from(pdf).toString('latin1');
    for (const name of ['VarTest_75wdth_700wght', 'VarTest_62.5wdth']) expect(text).toContain(name);
    const t = treeFonts(el);
    expect(t.warnings).toEqual([]);
    expect(t.fonts?.filter((f) => f.source === 'Var-Wide-100-900-50-100%')).toEqual([
      {
        id: 'Var-Wide-100-900-50-100%@wdth=62.5,wght=400',
        family: 'Var Wide',
        stretch: 62.5,
        source: 'Var-Wide-100-900-50-100%',
        variations: { wdth: 62.5, wght: 400 },
      },
      {
        id: 'Var-Wide-100-900-50-100%@wdth=75,wght=700',
        family: 'Var Wide',
        weight: 700,
        stretch: 75,
        source: 'Var-Wide-100-900-50-100%',
        variations: { wdth: 75, wght: 700 },
      },
    ]);
    expect(() => Font.register({ family: 'Var Bad', src: varFont, fontStretch: [100, 50] })).toThrow(
      'the fontStretch range [100, 50] goes from the narrower width',
    );
    expect(() =>
      Font.register({ family: 'Var Bad', src: varFont, fontStretch: 'squashed' as never }),
    ).toThrow('is not a width of 1% to 1000% or a CSS keyword');
  });

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

  it('reads a Blob or a File, and tells two apart by identity', async () => {
    const bytes = readFileSync(file('Inter-Regular.ttf'));
    const picked = new File([bytes], 'Picked.ttf');
    Font.register({ family: 'Blob Font', src: new Blob([bytes]) });
    Font.register({ family: 'File Font', src: picked });
    expect(() => Font.register({ family: 'File Font', src: picked })).not.toThrow();
    expect(() => Font.register({ family: 'File Font', src: new File([bytes], 'Picked.ttf') })).toThrow(
      'Font "File Font" 400 is already registered from the file Picked.ttf.',
    );
    await renderToBuffer(<Doc family="File Font" />);
    expect(fonts.has('Blob-Font-400')).toBe(true);
    expect(fonts.has('File-Font-400')).toBe(true);
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

  it('registers a colour emoji font without outlines, and falls back to it for emoji', async () => {
    Font.register({ family: 'Emoji Test', src: file('EmojiTest-CBDT.ttf') });
    const pdf = await renderToBuffer(
      <Document fallbackFonts={['NotoSans-Regular', 'Emoji-Test-400']}>
        <Page>
          <Text>Thumbs up 👍 and a flag 🇫🇷</Text>
        </Page>
      </Document>,
      { compress: false },
    );
    const text = Buffer.from(pdf).toString('latin1');
    // Each emoji is an image (and its alpha mask), drawn by a glyph of a
    // Type 3 font; the font file is not embedded, only Inter's. The space
    // after the thumbs is the colour font's, which cannot draw it: it keeps
    // its text in a span.
    expect(text.match(/\/Subtype \/Image/g)).toHaveLength(4);
    expect(text).toContain('/Name /EmojiTestCBDT-Regular-Color0');
    expect(text.match(/\/FontFile/g)).toHaveLength(1);
    expect(text).toContain('/ActualText ( )');
  });

  it('fails the render with the family and source when a font cannot be loaded, once', async () => {
    Font.register({ family: 'Missing', src: '/nowhere/Missing.ttf' });
    const err = await renderToBuffer(<Doc family="Brand" />).catch((e) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect(err.code).toBe('INVALID_FONT');
    expect(err.message).toContain('Font "Missing" 400 (/nowhere/Missing.ttf)');
    // The failed face is dropped: the next render works, its text in it falls back with a
    // warning, as the guide says, and it can be registered again.
    await expect(renderToBuffer(<Doc family="Missing" />)).resolves.toBeInstanceOf(Uint8Array);
    expect(treeFonts(<Doc family="Missing" />).warnings).toEqual([
      'Font "Missing 400" is not registered; using Inter-Regular (Text in Page).',
    ]);
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
    // Family names match ignoring ASCII case.
    expect(() => Font.register({ family: 'BRAND', src: file('Inter-Regular.ttf'), fontWeight: 700 })).toThrow(
      'already registered',
    );
    expect(() => Font.register({ family: 'inter', src: file('Inter-Regular.ttf') })).toThrow('is bundled');
    expect(() => Font.register({ family: 'Brand', src, fontWeight: 'heavyish' as never })).toThrow(
      'not 1–1000 or a CSS weight name',
    );
    // A weight range is another face than a static weight in it.
    Font.register({ family: 'Ranged', src: file('Inter-Regular.ttf') });
    expect(() =>
      Font.register({ family: 'Ranged', src: file('Inter-Regular.ttf'), fontWeight: [100, 900] }),
    ).not.toThrow();
  });

  it('compares byte sources by their bytes, not their length', async () => {
    Font.register({ family: 'Same Size', src: new Uint8Array(1000).fill(1) });
    expect(() => Font.register({ family: 'Same Size', src: new Uint8Array(1000).fill(1) })).not.toThrow();
    expect(() => Font.register({ family: 'Same Size', src: new Uint8Array(1000).fill(2) })).toThrow(
      'Font "Same Size" 400 is already registered from 1000 bytes.',
    );
    // They are not a font: the next render fails on them once.
    await expect(renderToBuffer(<Doc family="Brand" />)).rejects.toMatchObject({ code: 'INVALID_FONT' });
  });

  it('refuses a family whose font id another family has', () => {
    const src = file('Inter-Regular.ttf');
    Font.register({ family: 'Id Clash', src });
    expect(() => Font.register({ family: 'Id-Clash', src })).toThrow(
      'Font "Id-Clash" 400 would have the font id Id-Clash-400, which "Id Clash" 400 has. Use another family name.',
    );
    // Only a face of the same weight and style has the same id.
    expect(() => Font.register({ family: 'Id-Clash', src, fontStyle: 'italic' })).not.toThrow();
    expect(() => Font.register({ family: 'Id-Clash', src, fontWeight: 700 })).not.toThrow();
  });

  it('refuses a face the bundled fonts already have, and adds the faces they lack', async () => {
    expect(() => Font.register({ family: 'Inter', src: file('RobotoMono-Regular.ttf') })).toThrow(
      'Font "Inter" 400 is bundled (Inter-Regular). Register yours under another family name.',
    );
    expect(() =>
      Font.register({ family: 'Inter', src: file('Inter-Bold.ttf'), fontWeight: 700, fontStyle: 'italic' }),
    ).not.toThrow();
    await renderToBuffer(<Doc family="Inter" weight={700} italic />);
    expect(treeFonts(<Doc family="Inter" weight={700} italic />).warnings).toEqual([]);
  });

  it('refuses a src that is not a path, a URL or bytes when it is registered', async () => {
    for (const src of [null, 42, {}]) {
      expect(() => Font.register({ family: 'Bad Src', src: src as never })).toThrow(
        'Font "Bad Src": src must be a path, a URL, the bytes (Uint8Array or ArrayBuffer) or a Blob',
      );
      expect(() =>
        Font.register({
          family: 'Bad Src',
          fonts: [{ src: file('Inter-Bold.ttf'), fontWeight: 700 }, { src: src as never }],
        }),
      ).toThrow('src must be a path');
    }
    // Nothing of a refused registration is kept.
    await expect(renderToBuffer(<Doc family="Brand" />)).resolves.toBeInstanceOf(Uint8Array);
    expect(fonts.has('Bad-Src-700')).toBe(false);
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
