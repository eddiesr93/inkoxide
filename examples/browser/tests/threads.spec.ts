/**
 * The page, cross-origin isolated: the engine renders on a pool of threads in Web Workers, in
 * Chromium, Firefox and WebKit, the same PDF as Node and as one thread.
 */
import { expect, test } from '@playwright/test';

import { ISOLATED_PORT } from '../playwright.config.ts';
import { ISOLATION } from '../vite.config.ts';
import { call, instrumentWorkers, node, open } from './page.ts';

test.use({ baseURL: `http://localhost:${ISOLATED_PORT}` });

test('renders on a pool of threads the bytes Node renders', async ({ page }) => {
  const violations = await open(page);
  expect(await page.evaluate(() => globalThis.crossOriginIsolated)).toBe(true);
  const cores = await page.evaluate(() => navigator.hardwareConcurrency);
  expect(await call(page, 'threads')).toBe(Math.min(cores, 8));
  expect(await call(page, 'invoiceDigest')).toBe(node.invoice);
  expect(await call(page, 'letterDigest')).toBe(node.letter);
  expect(violations).toEqual([]);
});

test('serves the page cross-origin isolated', async ({ page }) => {
  const response = await page.goto('/');
  for (const [name, value] of Object.entries(ISOLATION)) {
    expect(response?.headers()[name.toLowerCase()]).toBe(value);
  }
});

test('a signal ends the worker of a render on the pool, and the next one renders', async ({ page }) => {
  const workers = await instrumentWorkers(page);
  await open(page);
  const before = await workers();
  expect(await call(page, 'abortedRender')).toBe('stopped');
  expect(await call(page, 'invoiceDigest')).toBe(node.invoice);
  expect(await workers()).toBe(before + 1);
});

test('timeoutMs stops a render on the pool, and the next one renders', async ({ page }) => {
  await open(page);
  expect(await call(page, 'timedOutRender')).toBe('TimeoutError');
  expect(await call(page, 'invoiceDigest')).toBe(node.invoice);
});

test('a bug on a thread of the pool fails the render, and the next one renders', async ({ page }) => {
  const workers = await instrumentWorkers(page);
  await open(page);
  test.skip((await call(page, 'threads')) < 2, 'one core: no pool');
  const before = await workers();
  await page.evaluate(() => {
    (globalThis as unknown as { inkoxidePoolPanic: boolean }).inkoxidePoolPanic = true;
  });
  expect(await call(page, 'failedRender')).toBe('INTERNAL');
  expect(await call(page, 'invoiceDigest')).toBe(node.invoice);
  expect(await workers()).toBe(before + 1);
});
