import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vite';

const page = (name: string) => fileURLToPath(new URL(`${name}.html`, import.meta.url));

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
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
    rolldownOptions: { input: { inkoxide: page('inkoxide'), 'react-pdf': page('react-pdf') } },
  },
});
