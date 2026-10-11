/** `Font.register`: families from paths, URLs and bytes, loaded before the next render. */
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import {
  assetsDir,
  Document,
  Font,
  fonts,
  Page,
  RenderError,
  renderTemplate,
  renderToBuffer,
  Text,
} from '../src/index.ts';

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
    else res.writeHead(404).end();
  });
  await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
  base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(() => new Promise<void>((ok) => server.close(() => ok())));

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

  it('fetches a font from a URL', async () => {
    Font.register({ family: 'Remote', src: `${base}/Inter-Regular.ttf` });
    await renderToBuffer(<Doc family="Remote" />);
    expect(fonts.has('Remote-400')).toBe(true);
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
