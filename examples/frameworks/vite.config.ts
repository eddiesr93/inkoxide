import { inkoxideTailwind } from '@inkoxide/tailwind/plugin';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import vue from '@vitejs/plugin-vue';
import { defineConfig, type Plugin } from 'vite';

/**
 * The policy of the Vue and Svelte pages: the engine needs only 'wasm-unsafe-eval' and a worker of
 * its own origin, as on the React page of examples/browser.
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

/** The Angular page's: its components compile just in time, which needs 'unsafe-eval'. */
export const JIT_CSP = CSP.replace("'wasm-unsafe-eval'", "'wasm-unsafe-eval' 'unsafe-eval'");

/** Each page's policy on `vite preview`. */
const policies = (): Plugin => ({
  name: 'policies',
  configurePreviewServer(server) {
    server.middlewares.use((req, res, next) => {
      res.setHeader('Content-Security-Policy', req.url?.startsWith('/angular') ? JIT_CSP : CSP);
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
  ],
  resolve: {
    // The templates import from the main entries, as they would in Node: the browser entries here.
    alias: [
      { find: /^@inkoxide\/vue$/, replacement: '@inkoxide/vue/browser' },
      { find: /^@inkoxide\/svelte$/, replacement: '@inkoxide/svelte/browser' },
    ],
  },
  worker: { format: 'es' },
  build: {
    target: 'es2023',
    rolldownOptions: { input: { vue: 'vue.html', svelte: 'svelte.html', angular: 'angular.html' } },
  },
});
