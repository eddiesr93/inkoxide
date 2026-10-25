/**
 * Each page renders the invoice with its front end (Vue, Svelte, Angular) in a Web Worker, in
 * Chromium, Firefox and WebKit, the same PDF as React in Node; its classes come from the table the
 * Tailwind plugin made of the templates (.vue, .svelte, Angular's inline templates in .ts).
 */
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

import { expect, test } from '@playwright/test';

import { CSP, JIT_CSP } from '../vite.config.ts';
import { call, expected, open, PAGES } from './page.ts';

for (const page of PAGES) {
  test.describe(page, () => {
    test('renders in a worker the bytes React renders in Node, under its Content Security Policy', async ({
      page: tab,
    }) => {
      const problems = await open(tab, page, page === 'angular' ? JIT_CSP : CSP);
      expect(await call(tab, 'invoiceDigest')).toBe(expected);
      expect(problems).toEqual([]);
    });

    test('the download link downloads the PDF the viewer shows', async ({ page: tab }) => {
      await tab.goto(`/${page}.html`);
      const link = tab.locator('#download');
      await expect(link).toHaveAttribute('href', /^blob:/, { timeout: 30_000 });
      await expect(link).toHaveAttribute('download', 'invoice.pdf');
      const [download] = await Promise.all([tab.waitForEvent('download'), link.click()]);
      const file = readFileSync((await download.path()) ?? '');
      expect(createHash('sha256').update(file).digest('hex')).toBe(expected);
      await expect(tab.locator('iframe#viewer')).toHaveAttribute('src', /^blob:/);
    });
  });
}
