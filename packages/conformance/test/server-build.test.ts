/**
 * The templates in a server build, as a Nuxt app's server bundle and SvelteKit's endpoints have
 * them (Nuxt's server routes are Nitro's, which takes Vue templates as render functions): Vite
 * builds the code for Node (single-file components compiled for the client through
 * `@inkoxide/vue/vite`, Svelte's for the server), and the built module renders the PDF React
 * renders.
 */
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { renderToBuffer } from '@inkoxide/react';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import vue from '@vitejs/plugin-vue';
import { createElement } from 'react';
import { build } from 'vite';
import { afterAll, describe, expect, it } from 'vitest';

import Invoice, { sample } from '../../../examples/invoice.tsx';
import { pdfTemplates } from '../../vue/src/vite.ts';

// Inside the package, so that the built module finds the packages it imports.
const out = mkdtempSync(new URL('../.server-build-', import.meta.url).pathname);
const sha = (pdf: Uint8Array) => createHash('sha256').update(pdf).digest('hex');

afterAll(() => {
  if (!process.env.KEEP) rmSync(out, { recursive: true, force: true });
});

describe('a server build', () => {
  it('renders, with each front end, the PDF React renders', async () => {
    await build({
      configFile: false,
      logLevel: 'silent',
      root: new URL('..', import.meta.url).pathname,
      plugins: [pdfTemplates(vue()), svelte()],
      build: {
        ssr: 'server/entry.ts',
        outDir: out,
        rolldownOptions: {
          output: { entryFileNames: 'entry.mjs' },
          // As installed from npm, the packages stay external (Vite bundles workspace links), but
          // @inkoxide/svelte, whose components the server build compiles.
          external: (id) => /^(?:inkoxide|@inkoxide\/(?!svelte)[\w-]+|vue|svelte)(?:\/|$)/.test(id),
        },
      },
      ssr: { noExternal: ['@inkoxide/svelte'] },
    });
    const server = (await import(pathToFileURL(join(out, 'entry.mjs')).href)) as {
      vue(data: Record<string, unknown>): Promise<Uint8Array>;
      svelte(data: Record<string, unknown>): Promise<Uint8Array>;
    };
    const expected = sha(
      await renderToBuffer(createElement(Invoice, sample), { creationDate: '2026-10-25T00:00:00Z' }),
    );
    expect(sha(await server.vue({ ...sample }))).toBe(expected);
    expect(sha(await server.svelte({ ...sample }))).toBe(expected);
  }, 120_000);
});
