// What the tests call on every page: the digest of the invoice the page's front end renders, and
// the threads the engine renders with.
import type { BrowserRenderOptions } from '@inkoxide/vue/browser';

/** The options of every render: a fixed creation date, so that the bytes are the same as Node's. */
export const OPTIONS: BrowserRenderOptions = { creationDate: '2026-10-08T00:00:00Z' };

/** What the tests call on every page. */
export interface Hooks {
  /** The SHA-256 of the invoice the page renders. */
  invoiceDigest(): Promise<string>;
  /** The threads the engine renders with (1 without cross-origin isolation). */
  threads(): Promise<number>;
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes as Uint8Array<ArrayBuffer>);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Puts the hooks on the page, for what `render` makes, and the engine of the page's front end
 * (`engineStatus` of its browser entry).
 */
export function expose(
  render: () => Promise<Uint8Array>,
  engineStatus: () => { threads?: number } | undefined,
): void {
  const hooks: Hooks = {
    invoiceDigest: async () => sha256(await render()),
    threads: async () => engineStatus()?.threads ?? 0,
  };
  Object.assign(globalThis, { inkoxide: hooks });
}
