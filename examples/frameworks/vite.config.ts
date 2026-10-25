import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import vue from '@vitejs/plugin-vue';
import { defineConfig, type Plugin } from 'vite';

import { cdn, cdnCsp, isolated } from '../browser/vite.config.ts';

/**
 * The policy of the Vue and Svelte pages: the engine needs only 'wasm-unsafe-eval' and a worker of
 * its own origin, as on the React page of examples/browser. With INKOXIDE_ISOLATED set, the pages
 * are cross-origin isolated too, and the engine renders on a pool of threads; with INKOXIDE_CDN,
 * the build's scripts and assets are another origin's, as from a CDN (CDN_PORT, `assets`), and the
 * pages (`page`) are cross-origin isolated (tests/cdn.spec.ts). As examples/browser does.
 */
export const CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob:",
  "frame-src 'self' blob:",
  "object-src 'none'",
  "base-uri 'none'",
].join('; ');

/** A policy for the Angular page: its components compile just in time, which needs 'unsafe-eval'. */
const jit = (csp: string) => csp.replace("'wasm-unsafe-eval'", "'wasm-unsafe-eval' 'unsafe-eval'");

/** The Angular page's policy. */
export const JIT_CSP = jit(CSP);

/** The origin that serves the scripts and assets of the build with INKOXIDE_CDN. */
export const CDN_PORT = 4185;
const CDN = `http://localhost:${CDN_PORT}`;

/** The policies of the pages with INKOXIDE_CDN: the Vue and Svelte pages', and the Angular page's. */
export const CDN_CSP = cdnCsp(CDN);
export const CDN_JIT_CSP = jit(CDN_CSP);

const { INKOXIDE_CDN } = process.env;

/** Each page's policy on `vite preview`. */
const policies = (): Plugin => ({
  name: 'policies',
  configurePreviewServer(server) {
    server.middlewares.use((req, res, next) => {
      const angular = req.url?.startsWith('/angular');
      const policy = INKOXIDE_CDN ? (angular ? CDN_JIT_CSP : CDN_CSP) : angular ? JIT_CSP : CSP;
      res.setHeader('Content-Security-Policy', policy);
      next();
    });
  },
});

const conformance = '../../packages/conformance';

export default defineConfig({
  plugins: [
    vue(),
    svelte(),
    // The classes of the templates, in .vue, .svelte and the inline templates of Angular's .ts.
    inkoxideTailwind({
      sources: ['src', `${conformance}/vue`, `${conformance}/svelte`, `${conformance}/angular`],
    }),
    policies(),
    ...(process.env.INKOXIDE_ISOLATED || INKOXIDE_CDN === 'page' ? [isolated()] : []),
    ...(INKOXIDE_CDN === 'assets' ? [cdn()] : []),
  ],
  ...(INKOXIDE_CDN ? { base: `${CDN}/` } : {}),
  resolve: {
    // The templates import from the main entries, as they would in Node: the browser entries here.
    alias: [
      { find: /^@inkoxide\/vue$/, replacement: '@inkoxide/vue/browser' },
      { find: /^@inkoxide\/svelte$/, replacement: '@inkoxide/svelte/browser' },
      { find: /^@inkoxide\/angular$/, replacement: '@inkoxide/angular/browser' },
    ],
  },
  worker: { format: 'es' },
  build: {
    target: 'es2023',
    rolldownOptions: { input: { vue: 'vue.html', svelte: 'svelte.html', angular: 'angular.html' } },
    ...(INKOXIDE_CDN ? { outDir: 'dist-cdn' } : {}),
  },
});
