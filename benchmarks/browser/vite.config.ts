import { fileURLToPath } from 'node:url';

import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';
import { defineConfig, type Plugin } from 'vite';

const page = (name: string) => fileURLToPath(new URL(`${name}.html`, import.meta.url));

/**
 * Serves the page cross-origin isolated, so that the engine can render on a pool of threads
 * (`?threads=`): the headers on every response, a 304 included, which WebKit needs.
 */
const isolated = (): Plugin => ({
  name: 'isolated',
  configurePreviewServer(server) {
    server.middlewares.use((_req, res, next) => {
      res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
      res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
      next();
    });
  },
});

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  // The documents' classes come from a table built here, not from Tailwind's compiler; they are
  // in benchmarks/documents.
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
