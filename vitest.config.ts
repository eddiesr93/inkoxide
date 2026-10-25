import { svelte } from '@sveltejs/vite-plugin-svelte';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

import { pdfTemplates } from './packages/vue/src/vite.ts';

// The single-file components of the Vue tests and the conformance suite, compiled for Inkoxide's
// renderer, as `@inkoxide/vue/vite` does in a server build; Svelte's, for the server in Node and
// for the client in the DOM tests.
const plugins = [pdfTemplates(vue()), svelte()];
const timeouts = { testTimeout: 30_000, hookTimeout: 60_000 };

export default defineConfig({
  plugins,
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'node',
          include: ['packages/*/test/**/*.test.{ts,tsx}'],
          exclude: ['packages/*/test/**/*.dom.test.ts'],
          environment: 'node',
          ...timeouts,
        },
      },
      {
        // As in a browser: the `browser` exports of the packages (Svelte's client runtime).
        extends: true,
        resolve: { conditions: ['browser'] },
        test: {
          name: 'dom',
          include: ['packages/*/test/**/*.dom.test.ts'],
          environment: 'happy-dom',
          ...timeouts,
        },
      },
    ],
  },
});
