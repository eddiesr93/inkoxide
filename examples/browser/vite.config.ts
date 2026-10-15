import { defineConfig } from 'vite';

/**
 * Served without cross-origin isolation, under a strict Content Security Policy: the engine needs
 * only 'wasm-unsafe-eval' and a worker of its own origin.
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

export default defineConfig({
  resolve: {
    // The invoice imports from `@inkoxide/render`, as it would in Node: the browser entry here.
    alias: [{ find: /^@inkoxide\/render$/, replacement: '@inkoxide/render/browser' }],
  },
  worker: { format: 'es' },
  build: { target: 'es2023' },
  preview: { headers: { 'Content-Security-Policy': CSP } },
});
