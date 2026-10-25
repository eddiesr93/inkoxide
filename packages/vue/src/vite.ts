// `@inkoxide/vue/vite`: compiles the single-file components of PDF templates for the client in a
// server build too. A server build (Nuxt's server routes, Vite's SSR, Vitest in Node) compiles
// `.vue` files to render HTML strings, which Inkoxide's renderer cannot use: it renders the
// components itself, as a browser would.
//
// ```ts
// import vue from '@vitejs/plugin-vue';
// import { pdfTemplates } from '@inkoxide/vue/vite';
//
// export default defineConfig({ plugins: [pdfTemplates(vue(), /\/pdf\//)] });
// ```
export { pdfTemplates, type TemplateFilter, type VuePluginLike } from './pdf-templates.ts';
