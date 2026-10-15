/**
 * The page renders in a Web Worker, in Chromium, Firefox and WebKit, under a strict Content
 * Security Policy and without cross-origin isolation, the same PDF as Node.
 */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { expect, type Page, test } from '@playwright/test';

import { CSP } from '../vite.config.ts';

/** The SHA-256 of the invoice rendered in Node: once for every browser. */
const expected = execFileSync('tsx', [fileURLToPath(new URL('render-node.tsx', import.meta.url))], {
  encoding: 'utf8',
}).trim();

interface Hooks {
  invoiceDigest(): Promise<string>;
  abortedRender(): Promise<string>;
}

/** The page once it has rendered, with the Content Security Policy problems it logged. */
async function open(page: Page): Promise<string[]> {
  const violations: string[] = [];
  page.on('console', (m) => {
    if (/content.security.policy|CSP/i.test(m.text())) violations.push(m.text());
  });
  page.on('pageerror', (e) => violations.push(`page error: ${e.message}`));
  const response = await page.goto('/');
  expect(response?.headers()['content-security-policy']).toBe(CSP);
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  return violations;
}

const call = (page: Page, hook: keyof Hooks) =>
  page.evaluate((h) => (globalThis as unknown as { inkoxide: Hooks }).inkoxide[h](), hook);

test('renders in a worker the bytes Node renders', async ({ page }) => {
  const violations = await open(page);
  expect(await call(page, 'invoiceDigest')).toBe(expected);
  expect(violations).toEqual([]);
});

test('needs no cross-origin isolation', async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => globalThis.crossOriginIsolated)).toBe(false);
});

test('a signal ends the worker of a render, and the next one renders', async ({ page }) => {
  // Counts the workers, and stops the render once its worker has it.
  await page.addInitScript(() => {
    const scope = globalThis as unknown as { workers: number; Worker: typeof Worker };
    const Base = scope.Worker;
    scope.workers = 0;
    scope.Worker = class extends Base {
      constructor(...args: ConstructorParameters<typeof Worker>) {
        super(...args);
        scope.workers++;
      }
      override postMessage(message: unknown, options?: Transferable[] | StructuredSerializeOptions) {
        super.postMessage(message, options as StructuredSerializeOptions);
        if ((message as { op?: string }).op === 'render')
          setTimeout(() => dispatchEvent(new Event('inkoxide:stop')));
      }
    };
  });
  await open(page);
  const workers = () => page.evaluate(() => (globalThis as unknown as { workers: number }).workers);
  expect(await workers()).toBe(1);
  expect(await call(page, 'abortedRender')).toBe('stopped');
  expect(await call(page, 'invoiceDigest')).toBe(expected);
  expect(await workers()).toBe(2);
});
