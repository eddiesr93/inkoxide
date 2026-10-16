// `@inkoxide/render/browser`: the components and rendering in a browser, with the engine as
// WebAssembly (`@inkoxide/wasm`) in a Web Worker. The API of the main entry without what needs
// Node (renderToFile, renderToStream, image files), with renderToBlob, the React helpers usePdf,
// PdfDownloadLink and PdfViewer, and generate for templates.
// test/exports.test.ts pins the list.
// In a browser: the engine in a worker, and the bundled assets fetched.
import './browser-host.ts';

export {
  type BrowserOptions,
  type BrowserRenderOptions,
  clearCaches,
  init,
  renderToBlob,
  renderToBuffer,
} from './browser-api.ts';
export * from './components.ts';
export {
  PdfDownloadLink,
  type PdfDownloadLinkProps,
  PdfViewer,
  type PdfViewerProps,
  usePdf,
  type UsePdfResult,
} from './helpers.tsx';
export { countPages, type EngineStatus, type EngineVersion, engineStatus, version } from './engine.ts';
export { Font, type FontRegistration, type FontSource, type FontSrc, type FontWeight } from './font.ts';
export { type GenerateOptions, generate, type RenderToBufferOptions, type TemplateLike } from './pipeline.ts';
export {
  type DataIssue,
  type RemoteImageOptions,
  RenderError,
  type RenderErrorCode,
  type RenderOpts,
  type RenderResult,
  type Timings,
} from './types.ts';
