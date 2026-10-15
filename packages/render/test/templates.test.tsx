/**
 * `@inkoxide/render/templates`: templates served by id. The first tests run before anything in
 * this file starts the engine (Vitest gives each test file a fresh module graph).
 */
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import {
  countPages,
  Document,
  engineStatus,
  Font,
  Footer,
  Image,
  type ImageSrc,
  init,
  Page,
  PageBreak,
  RenderError,
  Text,
  version,
  View,
} from '../src/index.ts';
import { assetsDir } from '../src/node-host.ts';
import {
  generate,
  parseData,
  renderDocument,
  renderSettled,
  renderTree,
  type TemplateLike,
} from '../src/templates.ts';

const styled: TemplateLike = {
  meta: { id: 'styled' },
  schema: z.object({}),
  Template: () => (
    <Document>
      <Page>
        <Text style={{ fontWeight: 700 }}>Hello</Text>
      </Page>
    </Document>
  ),
};

const classes: TemplateLike = {
  meta: { id: 'classes' },
  schema: z.object({}),
  Template: () => (
    <Document>
      <Page>
        <View className="p-4">
          <Text className="font-bold">Hello</Text>
        </View>
      </Page>
    </Document>
  ),
};

describe('renderTree', () => {
  it('asks for init() when it runs before the engine has started', async () => {
    for (const t of [styled, classes]) {
      const err = (() => {
        try {
          renderTree(t, {});
        } catch (e) {
          return e;
        }
      })();
      expect(err).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({ code: 'INTERNAL' });
      expect((err as RenderError).message).toContain('call `await init()` first');
    }
    await init();
    for (const t of [styled, classes]) {
      const tree = renderTree(t, {});
      expect(tree.warnings).toEqual([]);
      expect((await renderSettled(tree, {})).pages).toBe(1);
    }
  });

  it('asks for init() again while fonts registered since are not loaded', async () => {
    Font.register({ family: 'Late', src: readFileSync(join(assetsDir(), 'fonts', 'Inter-Bold.ttf')) });
    expect(() => renderTree(styled, {})).toThrow('call `await init()` first');
    await init();
    expect(renderTree(styled, {}).warnings).toEqual([]);
  });
});

const logo = readFileSync(resolve(import.meta.dirname, '../../../fixtures/images/logo.png'));

const Logo = ({ src }: { src: ImageSrc }) => (
  <Document>
    <Page>
      <Image src={src} className="h-10 w-10" />
    </Page>
  </Document>
);

const template = (src: ImageSrc, images?: Record<string, Uint8Array>): TemplateLike => ({
  meta: { id: 'logo' },
  schema: z.object({}),
  Template: () => <Logo src={src} />,
  images,
});

describe('parseData', () => {
  const invoice: TemplateLike = {
    meta: { id: 'invoice' },
    schema: z.object({
      number: z.string(),
      items: z.array(z.object({ name: z.string(), qty: z.number() })),
      note: z.string().default(''),
    }),
    Template: () => null,
  };

  it('returns the data parsed, with the defaults applied', () => {
    const data = { number: 'A-1', items: [{ name: 'Tea', qty: 2 }] };
    expect(parseData(invoice, data)).toEqual({ ...data, note: '' });
  });

  it('fails with INVALID_DATA, every issue and its path', () => {
    const err = (() => {
      try {
        parseData(invoice, { number: 7, items: [{ name: 'Tea' }] });
      } catch (e) {
        return e;
      }
    })() as RenderError;
    expect(err).toBeInstanceOf(RenderError);
    expect(err).toMatchObject({ code: 'INVALID_DATA', status: 400 });
    expect(err.message).toBe(
      'The data does not match the "invoice" schema: number: Expected a string, got a number.',
    );
    expect(err.issues).toEqual([
      { path: 'number', message: 'Expected a string, got a number', code: 'invalid_type' },
      { path: 'items[0].qty', message: 'Missing: expected a number', code: 'invalid_type' },
    ]);
    // As a server sends it: the fields that are set, without status or the stack.
    expect(JSON.parse(JSON.stringify(err))).toEqual({
      code: 'INVALID_DATA',
      message: err.message,
      issues: err.issues,
    });
  });

  it('names the root when the data itself is wrong', () => {
    expect(() => parseData(invoice, null)).toThrow(
      'The data does not match the "invoice" schema: (root): Expected an object, got null.',
    );
  });
});

describe('RenderError', () => {
  it('turns into JSON with only the fields that are set', () => {
    const e = new RenderError('TEMPLATE_ERROR', 'Bad', {
      path: 'pages[0]',
      component: 'Invoice',
      componentStack: '\n    in Invoice',
      className: 'p-x',
      engineCode: 'X',
      cause: new Error('inner'),
    });
    expect(e.status).toBe(400);
    expect(JSON.parse(JSON.stringify(e))).toEqual({
      code: 'TEMPLATE_ERROR',
      message: 'Bad',
      path: 'pages[0]',
      component: 'Invoice',
      componentStack: '\n    in Invoice',
      className: 'p-x',
      engineCode: 'X',
    });
    expect(JSON.parse(JSON.stringify(new RenderError('UNKNOWN_TEMPLATE', 'No template "x".')))).toEqual({
      code: 'UNKNOWN_TEMPLATE',
      message: 'No template "x".',
    });
    expect(new RenderError('UNKNOWN_TEMPLATE', '').status).toBe(404);
    expect(new RenderError('IMAGE_FETCH_FAILED', '').status).toBe(502);
    expect(new RenderError('INTERNAL', '').status).toBe(500);
  });
});

describe('health checks', () => {
  it('report the engine version and, once started, its status', async () => {
    const v = version();
    expect(v).toMatchObject({ engine: expect.any(String), schema: 1, gitSha: expect.any(String) });
    const status = await init();
    expect(engineStatus()).toBe(status);
    expect(status.version).toEqual(v);
    expect(status.fonts).toEqual(expect.arrayContaining(['Inter-Regular', 'Inter-Bold']));
    expect(status.hyphenation).toEqual({ en: true });
    expect(status.warnings).toEqual([]);
  });
});

describe('generate', () => {
  it('counts each stage once: rebuilds for the page count are tree time, not engine time', async () => {
    const spin = (ms: number) => {
      const end = performance.now() + ms;
      while (performance.now() < end);
    };
    const folio: TemplateLike = {
      meta: { id: 'folio' },
      schema: z.object({}),
      Template: () => (
        <Document>
          <Page>
            <Footer>
              <Text
                render={({ pageNumber, totalPages }) => {
                  spin(10);
                  return `${pageNumber} / ${totalPages}`;
                }}
              />
            </Footer>
            <Text>One</Text>
            <PageBreak />
            <Text>Two</Text>
          </Page>
        </Document>
      ),
    };
    const { passes, timings } = await generate(folio, {}, { validate: false });
    expect(passes).toBe(2);
    expect(timings.validateMs).toBe(0);
    // The first build and the rebuild call the render prop once per page: 1 + 2 times.
    expect(timings.treeMs).toBeGreaterThanOrEqual(30);
    const { reactMs, treeMs, validateMs, engineMs, fetchMs, totalMs } = timings;
    expect(reactMs + treeMs + validateMs + engineMs + fetchMs).toBeLessThanOrEqual(totalMs + 0.5);
  });
});

describe('renderDocument', () => {
  it('takes images from its images argument and from options.images', async () => {
    const { document } = await generate(template('logo', { logo }), {});
    for (const [images, opts] of [
      [{ logo }, {}],
      [{}, { images: { logo } }],
    ] as const) {
      const { pdf, pages } = await renderDocument(document, images, opts);
      expect(pages).toBe(1);
      expect(countPages(pdf)).toBe(1);
    }
    await expect(renderDocument(document)).rejects.toMatchObject({
      code: 'ENGINE_REJECTED',
      engineCode: 'UNKNOWN_IMAGE',
    });
  });

  it('renders a stored result again with the images of the result', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'inkoxide-templates-'));
    try {
      writeFileSync(join(dir, 'logo.png'), logo);
      const sources: ImageSrc[] = [logo, () => logo, './logo.png', 'logo'];
      for (const src of sources) {
        const result = await generate(template(src, { logo }), {}, { localImages: { root: dir } });
        const stored = JSON.parse(JSON.stringify(result.document)) as typeof result.document;
        expect(Object.keys(result.images).length).toBeGreaterThan(0);
        const again = await renderDocument(stored, result.images);
        expect(Buffer.from(again.pdf).equals(Buffer.from(result.pdf))).toBe(true);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
