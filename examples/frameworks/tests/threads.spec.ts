/**
 * Each page, cross-origin isolated: the engine renders on a pool of threads in Web Workers, in
 * Chromium, Firefox and WebKit, the same PDF as React in Node.
 */
import { expect, test } from '@playwright/test';

import { ISOLATED_PORT } from '../playwright.config.ts';
import { CSP, JIT_CSP } from '../vite.config.ts';
import { call, expected, open, PAGES } from './page.ts';

test.use({ baseURL: `http://localhost:${ISOLATED_PORT}` });

for (const page of PAGES) {
  test(`${page} renders on a pool of threads the bytes React renders in Node`, async ({ page: tab }) => {
    const problems = await open(tab, page, page === 'angular' ? JIT_CSP : CSP);
    expect(await tab.evaluate(() => globalThis.crossOriginIsolated)).toBe(true);
    const cores = await tab.evaluate(() => navigator.hardwareConcurrency);
    expect(await call(tab, 'threads')).toBe(Math.min(cores, 8));
    expect(await call(tab, 'invoiceDigest')).toBe(expected);
    expect(problems).toEqual([]);
  });
}
