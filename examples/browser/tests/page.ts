/** What the specs share: the page, its hooks (src/main.tsx), and the PDFs Node renders. */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { expect, type Page } from '@playwright/test';

import { CSP } from '../vite.config.ts';

/** The SHA-256 of the invoice and of the letter rendered in Node: once for every browser. */
export const node = JSON.parse(
  execFileSync('tsx', [fileURLToPath(new URL('render-node.tsx', import.meta.url))], { encoding: 'utf8' }),
) as { invoice: string; letter: string };

export interface Hooks {
  invoiceDigest(): Promise<string>;
  letterDigest(): Promise<string>;
  abortedRender(): Promise<string>;
  failedRender(): Promise<string>;
  threads(): Promise<number>;
  timedOutRender(): Promise<string>;
}

/** The page once it has rendered, with the Content Security Policy problems it logged. */
export async function open(page: Page): Promise<string[]> {
  const violations: string[] = [];
  page.on('console', (m) => {
    if (/content.security.policy|CSP/i.test(m.text())) violations.push(m.text());
  });
  page.on('pageerror', (e) => violations.push(`page error: ${e.message}`));
  const response = await page.goto('/');
  expect(response?.headers()['content-security-policy']).toBe(CSP);
  await expect(page.locator('#download [data-state]')).toHaveAttribute('data-state', 'ready', {
    timeout: 30_000,
  });
  return violations;
}

export const call = <H extends keyof Hooks>(page: Page, hook: H): ReturnType<Hooks[H]> =>
  page.evaluate(
    (h: keyof Hooks) => (globalThis as unknown as { inkoxide: Hooks }).inkoxide[h]() as Promise<unknown>,
    hook as keyof Hooks,
  ) as ReturnType<Hooks[H]>;

/**
 * Instruments the page's workers: counts them, and, through the engine's worker, once
 * `inkoxideStop` is set on the page, holds that worker busy before the next render and then stops
 * the render (it dispatches `inkoxide:stop`, which `abortedRender` waits for), so the stop always
 * comes while the worker has the render; or, once `inkoxidePoolPanic` is set, makes a thread of
 * the engine's pool panic just before the next render. Call it before `open`; it returns how many
 * workers the page has started.
 */
export async function instrumentWorkers(page: Page): Promise<() => Promise<number>> {
  await page.addInitScript(() => {
    const scope = globalThis as unknown as {
      workers: number;
      inkoxideStop?: boolean;
      inkoxidePoolPanic?: boolean;
      Worker: typeof Worker;
    };
    const Base = scope.Worker;
    scope.workers = 0;
    scope.Worker = class extends Base {
      constructor(...args: ConstructorParameters<typeof Worker>) {
        super(...args);
        scope.workers++;
      }
      override postMessage(message: unknown, options?: Transferable[] | StructuredSerializeOptions) {
        const render = (message as { op?: string }).op === 'render';
        const stop = render && scope.inkoxideStop === true;
        if (stop) {
          scope.inkoxideStop = false;
          // The worker answers in order: it is busy in this, and cannot finish the render, until
          // the stop ends it.
          super.postMessage({ id: -3, op: '__testHold', args: [] });
        }
        if (render && scope.inkoxidePoolPanic) {
          scope.inkoxidePoolPanic = false;
          // The worker answers in order: it is stuck in this before it gets to the render.
          super.postMessage({ id: -2, op: '__testPoolPanic', args: [] });
        }
        super.postMessage(message, options as StructuredSerializeOptions);
        if (stop) setTimeout(() => dispatchEvent(new Event('inkoxide:stop')));
      }
    };
  });
  return () => page.evaluate(() => (globalThis as unknown as { workers: number }).workers);
}
