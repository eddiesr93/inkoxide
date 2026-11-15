import { describe, expect, it } from 'vitest';

import { pdfTemplates } from '../src/vite.ts';

/** A plugin with the hooks of `@vitejs/plugin-vue`, which tells what each was called with. */
function plugin() {
  const calls: Array<{ hook: string; id: string; ssr: unknown }> = [];
  return {
    calls,
    name: 'vue',
    load(id: string, options?: { ssr?: boolean }) {
      calls.push({ hook: 'load', id, ssr: options?.ssr });
    },
    transform: {
      filter: { id: /\.vue$/ },
      handler(_code: string, id: string, options?: { ssr?: boolean }) {
        calls.push({ hook: 'transform', id, ssr: options?.ssr });
      },
    },
  };
}

type Load = (id: string, options?: { ssr?: boolean }) => void;
type Transform = { filter: unknown; handler(code: string, id: string, options?: { ssr?: boolean }): void };

describe('pdfTemplates', () => {
  it('compiles the files its filter matches for the client, by their id, in both hooks', () => {
    const vue = plugin();
    const wrapped = pdfTemplates(vue, /\/pdf\//);
    // The source of a template names no folder: the filter reads the id, not the code.
    const code = '<template><Document /></template>';
    for (const id of ['/app/src/pdf/Invoice.vue', '/app/src/App.vue']) {
      (wrapped.load as Load)(`${id}?vue&type=script&setup=true&lang.ts`, { ssr: true });
      (wrapped.transform as Transform).handler(code, id, { ssr: true });
    }
    expect(vue.calls.map((c) => [c.hook, c.ssr])).toEqual([
      ['load', false],
      ['transform', false],
      ['load', true],
      ['transform', true],
    ]);
    expect((wrapped.transform as Transform).filter).toBe(vue.transform.filter);
  });

  it('compiles every file for the client without a filter, and takes a function', () => {
    const all = plugin();
    (pdfTemplates(all).transform as Transform).handler('', '/app/src/App.vue', { ssr: true });
    expect(all.calls[0]?.ssr).toBe(false);

    const some = plugin();
    const wrapped = pdfTemplates(some, (file) => file.endsWith('Invoice.vue'));
    (wrapped.transform as Transform).handler('', '/app/src/Invoice.vue?vue&type=template', { ssr: true });
    (wrapped.transform as Transform).handler('', '/app/src/App.vue', { ssr: true });
    expect(some.calls.map((c) => c.ssr)).toEqual([false, true]);
  });
});
