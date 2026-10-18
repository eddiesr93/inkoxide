/**
 * The page with its scripts and assets from another origin, as from a CDN: the engine's worker
 * and its pool's are of the page's origin and import their scripts from there, and the engine
 * renders on a pool of threads the same PDF as Node.
 */
import { expect, test } from '@playwright/test';

import { CDN_PAGE_PORT } from '../playwright.config.ts';
import { CDN_CSP, CDN_PORT } from '../vite.config.ts';
import { call, node } from './page.ts';

test.use({ baseURL: `http://localhost:${CDN_PAGE_PORT}` });

test('renders on a pool of threads with the scripts of another origin', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => {
    if (m.type() === 'error' || m.type() === 'warning') problems.push(m.text());
  });
  page.on('pageerror', (e) => problems.push(`page error: ${e.message}`));
  const scripts: string[] = [];
  page.on('worker', (w) => scripts.push(w.url()));
  const response = await page.goto('/');
  expect(response?.headers()['content-security-policy']).toBe(CDN_CSP);
  await expect(page.locator('#download [data-state]')).toHaveAttribute('data-state', 'ready', {
    timeout: 30_000,
  });
  expect(await page.evaluate(() => globalThis.crossOriginIsolated)).toBe(true);
  const cores = await page.evaluate(() => navigator.hardwareConcurrency);
  expect(await call(page, 'threads')).toBe(Math.min(cores, 8));
  expect(await call(page, 'invoiceDigest')).toBe(node.invoice);
  expect(await call(page, 'letterDigest')).toBe(node.letter);
  // The page's own script came from the other origin, and its workers are the page's.
  expect(await page.evaluate(() => [...document.scripts].map((s) => new URL(s.src).port))).toContain(
    String(CDN_PORT),
  );
  expect(scripts.length).toBeGreaterThan(0);
  for (const url of scripts) expect(url).toMatch(new RegExp(`^blob:http://localhost:${CDN_PAGE_PORT}/`));
  expect(problems).toEqual([]);
});
