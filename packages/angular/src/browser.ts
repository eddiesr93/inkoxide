// `@inkoxide/angular/browser`: rendering in a browser, with the engine as WebAssembly
// (`@inkoxide/wasm`) in a Web Worker. The API of the main entry without what needs Node
// (renderToFile, image files), with renderToStream as a web stream, renderToBlob, injectPdf for
// templates of the app, and generate for templates served by id. test/exports.test.ts pins the list.
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
export { generate, renderToBlob, renderToBuffer, renderToStream } from './browser-render.ts';
export { injectPdf, type InjectPdfResult } from './helpers.ts';
export type { TemplateLike } from './template-render.ts';
