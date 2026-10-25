// `@inkoxide/render/browser`: the engine as WebAssembly (`@inkoxide/wasm`) in a Web Worker, for
// every front end: the main entry without what needs Node (`renderToFile`, image files), with
// `createRenderer` giving a Blob and a web stream. test/exports.test.ts pins the list.
// In a browser: the engine in a worker, and the bundled assets fetched.
import './browser-host.ts';

export {
  type BrowserOptions,
  type BrowserRenderer,
  type BrowserRenderOptions,
  clearCaches,
  createRenderer,
  type GenerateOptions,
  init,
} from './browser-api.ts';
export { type BlobRender, PdfRun, type PdfSnapshot, sameDocument } from './browser-pdf.ts';
export { countPages, type EngineStatus, type EngineVersion, engineStatus, version } from './engine.ts';
export { Font, type FontRegistration, type FontSource, type FontSrc, type FontWeight } from './font.ts';
export { type FrontEnd, renderDocument, type RenderToBufferOptions, type TemplateLike } from './pipeline.ts';
export {
  type DataIssue,
  type RemoteImageOptions,
  RenderError,
  type RenderErrorCode,
  type RenderOpts,
  type RenderResult,
  type Timings,
} from './types.ts';
