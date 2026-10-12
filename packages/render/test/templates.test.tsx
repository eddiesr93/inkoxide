/**
 * `@inkoxide/render/templates`: templates served by id. The first tests run before anything in
 * this file starts the engine (Vitest gives each test file a fresh module graph).
 */
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { assetsDir } from '../src/engine.ts';
import {
  countPages,
  Document,
  Font,
  Image,
  type ImageSrc,
  init,
  Page,
  RenderError,
  Text,
  View,
} from '../src/index.ts';
import { generate, renderDocument, renderSettled, renderTree, type TemplateLike } from '../src/templates.ts';

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
