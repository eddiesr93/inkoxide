import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_THEME_CSS } from '../src/index.ts';

const RED = [1, 0, 0, 1];
const BRAND = DEFAULT_THEME_CSS.replace(
  '--color-brand: rgb(12% 29% 60%);',
  '--color-brand: rgb(100% 0% 0%);',
);

/** A fresh `@inkoxide/tailwind`, with no theme loaded, whose default theme loads last. */
async function fresh() {
  vi.resetModules();
  const mod = await import('../src/index.ts');
  const { TailwindCompiler } = await import('../src/compile.ts');
  const load = TailwindCompiler.load.bind(TailwindCompiler);
  vi.spyOn(TailwindCompiler, 'load').mockImplementation(async (css, base) => {
    const compiler = await load(css, base);
    if (css === undefined) await new Promise((resolve) => setTimeout(resolve, 50));
    return compiler;
  });
  return mod;
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe('initTailwind', () => {
  it('keeps a theme being set up when a default call runs alongside it', async () => {
    expect(BRAND).not.toBe(DEFAULT_THEME_CSS);
    for (const order of ['default first', 'theme first']) {
      const { initTailwind, tw } = await fresh();
      const calls = [() => initTailwind(), () => initTailwind({ css: BRAND })];
      if (order === 'theme first') calls.reverse();
      await Promise.all(calls.map((call) => call()));
      expect(tw('bg-brand').backgroundColor, order).toEqual(RED);
    }
  });

  it('uses the theme set up last', async () => {
    const { initTailwind, tw } = await fresh();
    await Promise.all([
      initTailwind({ css: BRAND }),
      initTailwind({ css: DEFAULT_THEME_CSS }),
      initTailwind(),
    ]);
    expect(tw('bg-brand').backgroundColor).toEqual([0.12, 0.29, 0.6, 1]);
  });
});
