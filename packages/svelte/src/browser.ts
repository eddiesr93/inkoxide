// `@inkoxide/svelte/browser`: the components and rendering in a browser, with the engine as
// WebAssembly (`@inkoxide/wasm`) in a Web Worker. The API of the main entry without what needs
// Node (renderToFile, image files), with renderToStream as a web stream, renderToBlob, the Svelte
// helpers createPdf, PdfDownloadLink, PdfViewer and BlobProvider, and generate for templates.
// test/exports.test.ts pins the list.
export * from './core.ts';
export {
  type BrowserOptions,
  type BrowserRenderOptions,
  clearCaches,
  countPages,
  type DataIssue,
  type EngineStatus,
  type EngineVersion,
  engineStatus,
  Font,
  type FontRegistration,
  type FontSource,
  type FontSrc,
  type FontWeight,
  type GenerateOptions,
  init,
  type RemoteImageOptions,
  RenderError,
  type RenderErrorCode,
  type RenderOpts,
  type RenderResult,
  type RenderToBufferOptions,
  type Timings,
  version,
} from '@inkoxide/render/browser';
export { default as BlobProvider } from './browser/BlobProvider.svelte';
export { default as PdfDownloadLink } from './browser/PdfDownloadLink.svelte';
export { default as PdfViewer } from './browser/PdfViewer.svelte';
export { generate, renderToBlob, renderToBuffer, renderToStream } from './browser-render.ts';
export { renderTemplate, renderToTree } from './dom.ts';
export { createPdf, type PdfState } from './helpers.svelte.ts';
export type { TemplateLike } from './front-end.ts';
