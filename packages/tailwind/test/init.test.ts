import { fileURLToPath } from 'node:url';

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

describe('initTailwind options', () => {
  it('takes a theme that extends the default one', async () => {
    const { initTailwind, tw } = await fresh();
    await initTailwind({ css: `${DEFAULT_THEME_CSS}@theme { --color-accent: rgb(100% 0% 0%); }` });
    expect(tw('bg-accent').backgroundColor).toEqual(RED);
    expect(tw('font-sans').fontFamily).toEqual(['Inter', 'Noto Sans']);
  });

  it("resolves the theme's @imports from base", async () => {
    const { initTailwind, tw } = await fresh();
    await initTailwind({
      css: '@import "tailwindcss/theme";\n@import "./probe.css";',
      base: fileURLToPath(new URL('theme/', import.meta.url)),
    });
    expect(tw('bg-probe').backgroundColor).toEqual([0, 1, 0, 1]);
  });

  it('caches as many class strings as cacheSize, and warms the cache', async () => {
    const { initTailwind, tw } = await fresh();
    await initTailwind({ cacheSize: 2 });
    expect(tw.cacheSize()).toBe(0);
    tw.warm(['p-4', 'flex gap-2']);
    expect(tw.cacheSize()).toBe(2);
    // An unnormalised string is cached under itself and its normalised form.
    tw(' m-1  m-2 ');
    expect(tw.cacheSize()).toBe(2);
  });

  it('logs an unsupported class once when not strict', async () => {
    const { initTailwind, setTailwindStrict, tw } = await fresh();
    await initTailwind({ strict: true });
    expect(() => tw('p-2 shadow-md')).toThrow(/shadow-md/);
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    setTailwindStrict(false);
    expect(tw('p-2 shadow-md')).toEqual(tw('p-2'));
    tw('shadow-md m-1');
    expect(warn).toHaveBeenCalledTimes(1);
  });
});
