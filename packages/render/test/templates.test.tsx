/**
 * `@inkoxide/render/templates`: templates served by id. The first tests run before anything in
 * this file starts the engine (Vitest gives each test file a fresh module graph).
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { assetsDir } from '../src/engine.ts';
import { Document, Font, init, Page, RenderError, Text, View } from '../src/index.ts';
import { renderSettled, renderTree, type TemplateLike } from '../src/templates.ts';

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
