/**
 * The browser entry maps classes as the Node one does: Tailwind's own design system loader, with
 * the theme bundled, in place of `@tailwindcss/node`. Tailwind's compiler needs no Node, so this
 * runs here.
 */
import { describe, expect, it, vi } from 'vitest';

const CLASSES = [
  'p-4 m-2 text-sm font-bold',
  'flex flex-col gap-2 items-center justify-between',
  'bg-red-500 text-white border border-slate-300 rounded-lg',
  'w-1/2 h-[10vh] leading-tight tracking-wide',
  'font-sans font-mono text-brand bg-head',
  'grid grid-cols-3 col-span-2',
];
const UNITS = { parentFontSize: 12, fontSize: undefined, page: { width: 595, height: 842 } };

async function load(entry: '../src/index.ts' | '../src/browser.ts', css?: string) {
  vi.resetModules();
  const mod = await import(entry);
  await mod.initTailwind(css === undefined ? {} : { css });
  return mod;
}

describe('the browser entry', () => {
  it('maps classes as the Node entry does', async () => {
    const node = await load('../src/index.ts');
    const expected = CLASSES.map((c) => node.tw(c, UNITS));
    const browser = await load('../src/browser.ts');
    expect(CLASSES.map((c) => browser.tw(c, UNITS))).toEqual(expected);
    expect(browser.tw('font-sans').fontFamily).toEqual(['Inter', 'Noto Sans']);
  });

  it('takes a theme that imports Tailwind’s, and nothing else', async () => {
    const browser = await load(
      '../src/browser.ts',
      '@import "tailwindcss/theme";\n@theme { --color-accent: rgb(100% 0% 0%); }',
    );
    expect(browser.tw('bg-accent').backgroundColor).toEqual([1, 0, 0, 1]);
    await expect(load('../src/browser.ts', '@import "./probe.css";')).rejects.toThrow(
      /can import only "tailwindcss\/theme"/,
    );
  });
});
