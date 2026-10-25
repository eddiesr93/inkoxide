/**
 * Each page with its scripts and assets from another origin, as from a CDN: the engine's worker and
 * its pool's are of the page's origin and import their scripts from there, and the engine renders
 * on a pool of threads the same PDF as React in Node.
 */
import { expect, test } from '@playwright/test';

import { CDN_PAGE_PORT } from '../playwright.config.ts';
import { CDN_CSP, CDN_JIT_CSP, CDN_PORT } from '../vite.config.ts';
import { call, expected, open, PAGES } from './page.ts';

test.use({ baseURL: `http://localhost:${CDN_PAGE_PORT}` });

for (const page of PAGES) {
  test(`${page} renders on a pool of threads with the scripts of another origin`, async ({ page: tab }) => {
    const workers: string[] = [];
    tab.on('worker', (w) => workers.push(w.url()));
    const problems = await open(tab, page, page === 'angular' ? CDN_JIT_CSP : CDN_CSP);
    expect(await tab.evaluate(() => globalThis.crossOriginIsolated)).toBe(true);
    const cores = await tab.evaluate(() => navigator.hardwareConcurrency);
    expect(await call(tab, 'threads')).toBe(Math.min(cores, 8));
    expect(await call(tab, 'invoiceDigest')).toBe(expected);
    // The page's own script came from the other origin, and its workers are the page's.
    expect(await tab.evaluate(() => [...document.scripts].map((s) => new URL(s.src).port))).toContain(
      String(CDN_PORT),
    );
    expect(workers.length).toBeGreaterThan(0);
    for (const url of workers) expect(url).toMatch(new RegExp(`^blob:http://localhost:${CDN_PAGE_PORT}/`));
    expect(problems).toEqual([]);
  });
}
