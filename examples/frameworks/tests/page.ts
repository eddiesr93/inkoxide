/** What the specs share: the pages, their hooks (src/hooks.ts), and the PDF Node renders. */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { expect, type Page } from '@playwright/test';

import type { Hooks } from '../src/hooks.ts';

/** The SHA-256 of the invoice React renders in Node: once for every page and browser. */
export const expected = execFileSync(
  'tsx',
  [
    '--tsconfig',
    fileURLToPath(new URL('../../tsconfig.json', import.meta.url)),
    fileURLToPath(new URL('render-node.tsx', import.meta.url)),
  ],
  { encoding: 'utf8' },
);

/** The pages, each with its front end. */
export const PAGES = ['vue', 'svelte', 'angular'] as const;

/**
 * Opens a page, once it has rendered, with its Content Security Policy checked; gives the problems
 * it logged: policy violations and page errors.
 */
export async function open(tab: Page, page: (typeof PAGES)[number], policy: string): Promise<string[]> {
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
  return problems;
}

export const call = <H extends keyof Hooks>(tab: Page, hook: H): ReturnType<Hooks[H]> =>
  tab.evaluate(
    (h: keyof Hooks) => (globalThis as unknown as { inkoxide: Hooks }).inkoxide[h]() as Promise<unknown>,
    hook as keyof Hooks,
  ) as ReturnType<Hooks[H]>;
