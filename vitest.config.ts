import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

import { pdfTemplates } from './packages/vue/src/vite.ts';

export default defineConfig({
  // The single-file components of the Vue tests and the conformance suite, compiled for
  // Inkoxide's renderer, as `@inkoxide/vue/vite` does in a server build.
  plugins: [pdfTemplates(vue())],
  test: {
    include: ['packages/*/test/**/*.test.{ts,tsx}'],
    environment: 'node',
    testTimeout: 30_000,
    hookTimeout: 60_000,
  },
});
