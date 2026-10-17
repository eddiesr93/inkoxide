import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';
import { defineConfig, type Plugin } from 'vite';

/**
 * Served without cross-origin isolation, under a strict Content Security Policy: the engine needs
 * only 'wasm-unsafe-eval' and a worker of its own origin. With INKOXIDE_ISOLATED set, the page is
 * cross-origin isolated too (ISOLATION), and the engine renders on a pool of threads.
 */
export const CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "style-src 'self'",
  "img-src 'self' blob:",
  "frame-src 'self' blob:",
  "object-src 'none'",
  "base-uri 'none'",
].join('; ');

/** The headers that make a page cross-origin isolated, which threads need. */
export const ISOLATION = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};

/**
 * ISOLATION on every response of `vite preview`, a 304 included: `preview.headers` leaves them out
 * of a 304, and WebKit then refuses to start the engine's worker again from its cache.
 */
const isolated = (): Plugin => ({
  name: 'isolated',
  configurePreviewServer(server) {
    server.middlewares.use((_req, res, next) => {
      for (const [name, value] of Object.entries(ISOLATION)) res.setHeader(name, value);
      next();
    });
  },
});

export default defineConfig({
  // The page's classes come from a table built here, not from Tailwind's compiler.
  plugins: [inkoxideTailwind(), ...(process.env.INKOXIDE_ISOLATED ? [isolated()] : [])],
  resolve: {
    // The invoice imports from `@inkoxide/render`, as it would in Node: the browser entry here.
    alias: [{ find: /^@inkoxide\/render$/, replacement: '@inkoxide/render/browser' }],
  },
  worker: { format: 'es' },
  build: { target: 'es2023' },
  preview: { headers: { 'Content-Security-Policy': CSP } },
});
