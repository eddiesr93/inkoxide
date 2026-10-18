import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';
import { defineConfig, type Plugin } from 'vite';

/**
 * Served without cross-origin isolation, under a strict Content Security Policy: the engine needs
 * only 'wasm-unsafe-eval' and a worker of its own origin. With INKOXIDE_ISOLATED set, the page is
 * cross-origin isolated too (ISOLATION), and the engine renders on a pool of threads. With
 * INKOXIDE_CDN set, the build's scripts and assets are another origin's, as from a CDN (CDN_PORT,
 * `assets`), and the page (`page`) is cross-origin isolated (tests/cdn.spec.ts).
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

/** The origin that serves the scripts and assets of the build with INKOXIDE_CDN. */
export const CDN_PORT = 4181;
const CDN = `http://localhost:${CDN_PORT}`;

/**
 * The policy of the page whose scripts are another origin's: the engine's workers are then of the
 * page's origin, made from a `blob:` URL, and import their scripts from there.
 */
export const CDN_CSP = [
  "default-src 'self'",
  `script-src 'self' ${CDN} 'wasm-unsafe-eval'`,
  'worker-src blob:',
  `connect-src 'self' ${CDN}`,
  `style-src 'self' ${CDN}`,
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
 * of a 304, and WebKit then refuses to start the engine's worker again from its cache. The
 * browser benchmark serves its page with it too.
 */
export const isolated = (): Plugin => ({
  name: 'isolated',
  configurePreviewServer(server) {
    server.middlewares.use((_req, res, next) => {
      for (const [name, value] of Object.entries(ISOLATION)) res.setHeader(name, value);
      next();
    });
  },
});

/** The headers a CDN serves its files with, for a page of another origin, isolated or not. */
const cdn = (): Plugin => ({
  name: 'cdn',
  configurePreviewServer(server) {
    server.middlewares.use((_req, res, next) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Cross-Origin-Resource-Policy', 'cross-origin');
      next();
    });
  },
});

const { INKOXIDE_CDN } = process.env;

export default defineConfig({
  // The page's classes come from a table built here, not from Tailwind's compiler.
  plugins: [
    inkoxideTailwind(),
    ...(process.env.INKOXIDE_ISOLATED || INKOXIDE_CDN === 'page' ? [isolated()] : []),
    ...(INKOXIDE_CDN === 'assets' ? [cdn()] : []),
  ],
  ...(INKOXIDE_CDN ? { base: `${CDN}/` } : {}),
  resolve: {
    // The invoice imports from `@inkoxide/render`, as it would in Node: the browser entry here.
    alias: [{ find: /^@inkoxide\/render$/, replacement: '@inkoxide/render/browser' }],
  },
  worker: { format: 'es' },
  build: { target: 'es2023', ...(INKOXIDE_CDN ? { outDir: 'dist-cdn' } : {}) },
  preview: { headers: { 'Content-Security-Policy': INKOXIDE_CDN ? CDN_CSP : CSP } },
});
