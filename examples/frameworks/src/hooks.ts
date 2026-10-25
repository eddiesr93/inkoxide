// What the tests call on every page: the digest of the invoice the page's front end renders.
import type { BrowserRenderOptions } from '@inkoxide/vue/browser';

/** The options of every render: a fixed creation date, so that the bytes are the same as Node's. */
export const OPTIONS: BrowserRenderOptions = { creationDate: '2026-10-08T00:00:00Z' };

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes as Uint8Array<ArrayBuffer>);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** Puts the hook the tests call on the page: the SHA-256 of the invoice `render` makes. */
export function expose(render: () => Promise<Uint8Array>): void {
  Object.assign(globalThis, { inkoxide: { invoiceDigest: async () => sha256(await render()) } });
}
