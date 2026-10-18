import { fileURLToPath } from 'node:url';

import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';
import { defineConfig } from 'vite';

import { isolated } from '../../examples/browser/vite.config.ts';

const page = (name: string) => fileURLToPath(new URL(`${name}.html`, import.meta.url));

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  // The documents' classes come from a table built here, not from Tailwind's compiler; they are
  // in benchmarks/documents.
  // Served cross-origin isolated, so that the engine can render on a pool of threads (`?threads=`).
  plugins: [inkoxideTailwind({ sources: ['.', '../documents'] }), isolated()],
  resolve: {
    // The Inkoxide documents import from `@inkoxide/render`, as on the server: the browser entry.
    alias: [{ find: /^@inkoxide\/render$/, replacement: '@inkoxide/render/browser' }],
  },
  worker: { format: 'es' },
  build: {
    target: 'es2023',
    outDir: 'dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 4096,
    rolldownOptions: { input: { index: page('index') } },
  },
});
