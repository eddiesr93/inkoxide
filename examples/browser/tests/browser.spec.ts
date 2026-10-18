/**
 * The page renders in a Web Worker, in Chromium, Firefox and WebKit, under a strict Content
 * Security Policy and without cross-origin isolation, the same PDF as Node.
 */
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

import { expect, test } from '@playwright/test';

import { call, instrumentWorkers, node, open } from './page.ts';

const expected = node.invoice;

test('renders in a worker the bytes Node renders', async ({ page }) => {
  const violations = await open(page);
  expect(await call(page, 'invoiceDigest')).toBe(expected);
  expect(violations).toEqual([]);
});

test('PdfDownloadLink downloads the PDF PdfViewer shows', async ({ page }) => {
  await open(page);
  const link = page.locator('#download');
  await expect(link).toHaveAttribute('href', /^blob:/);
  await expect(link).toHaveAttribute('download', 'invoice.pdf');
  const [download] = await Promise.all([page.waitForEvent('download'), link.click()]);
  // Firefox under Playwright sometimes reports no name for a download of a blob: URL; the name
  // is the link's `download` attribute, checked above.
  expect(['invoice.pdf', '']).toContain(download.suggestedFilename());
  const file = readFileSync((await download.path()) ?? '');
  expect(createHash('sha256').update(file).digest('hex')).toBe(expected);
  await expect(page.locator('iframe#viewer')).toHaveAttribute('src', /^blob:/);
});

test('fetches hyphenation patterns and the fallback font as a document needs them', async ({ page }) => {
  await open(page);
  const fetched: string[] = [];
  page.context().on('request', (r) => fetched.push(new URL(r.url()).pathname));
  expect(await call(page, 'letterDigest')).toBe(node.letter);
  // Bundlers add a hash to the file names.
  expect(fetched.some((p) => /\/de(?:-[\w-]+)?\.bin$/.test(p))).toBe(true);
  expect(fetched.some((p) => /NotoSans-Regular[^/]*\.ttf$/.test(p))).toBe(true);
});

test('needs no cross-origin isolation', async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => globalThis.crossOriginIsolated)).toBe(false);
  expect(await call(page, 'threads')).toBe(1);
});

test('a signal ends the worker of a render, and the next one renders', async ({ page }) => {
  const workers = await instrumentWorkers(page);
  await open(page);
  const before = await workers();
  expect(await call(page, 'abortedRender')).toBe('stopped');
  expect(await call(page, 'invoiceDigest')).toBe(expected);
  expect(await workers()).toBe(before + 1);
});

test('timeoutMs stops a render that takes longer, and the next one renders', async ({ page }) => {
  await open(page);
  expect(await call(page, 'timedOutRender')).toBe('TimeoutError');
  expect(await call(page, 'invoiceDigest')).toBe(expected);
});
