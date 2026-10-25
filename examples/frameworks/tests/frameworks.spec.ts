/**
 * Each page renders the invoice with its front end (Vue, Svelte, Angular) in a Web Worker, in
 * Chromium, Firefox and WebKit, the same PDF as React in Node; its classes come from the table the
 * Tailwind plugin made of the templates (.vue, .svelte, Angular's inline templates in .ts).
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { expect, test } from '@playwright/test';

import { CSP, JIT_CSP } from '../vite.config.ts';

/** The SHA-256 of the invoice React renders in Node: once for every page and browser. */
const expected = execFileSync(
  'tsx',
  [
    '--tsconfig',
    fileURLToPath(new URL('../../tsconfig.json', import.meta.url)),
    fileURLToPath(new URL('render-node.tsx', import.meta.url)),
  ],
  {
    encoding: 'utf8',
  },
);

for (const [page, policy] of [
  ['vue', CSP],
  ['svelte', CSP],
  ['angular', JIT_CSP],
] as const) {
  test.describe(page, () => {
    test('renders in a worker the bytes React renders in Node, under its Content Security Policy', async ({
      page: tab,
    }) => {
      const problems: string[] = [];
      tab.on('console', (m) => {
        if (/content.security.policy|CSP/i.test(m.text())) problems.push(m.text());
      });
      tab.on('pageerror', (e) => problems.push(`page error: ${e.message}`));
      const response = await tab.goto(`/${page}.html`);
      expect(response?.headers()['content-security-policy']).toBe(policy);
      await expect(tab.locator('#download [data-state]')).toHaveAttribute('data-state', 'ready', {
        timeout: 30_000,
      });
      const digest = await tab.evaluate(() =>
        (
          globalThis as unknown as { inkoxide: { invoiceDigest(): Promise<string> } }
        ).inkoxide.invoiceDigest(),
      );
      expect(digest).toBe(expected);
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
