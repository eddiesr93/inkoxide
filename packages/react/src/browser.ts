// `@inkoxide/react/browser`: the components and rendering in a browser, with the engine as
// WebAssembly (`@inkoxide/wasm`) in a Web Worker. The API of the main entry without what needs
// Node (renderToFile, image files), with renderToStream as a web stream, renderToBlob, the React
// helpers usePdf, PdfDownloadLink, PdfViewer and BlobProvider, and generate for templates.
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
export {
  BlobProvider,
  type BlobProviderProps,
  PdfDownloadLink,
  type PdfDownloadLinkProps,
  PdfViewer,
  type PdfViewerProps,
  usePdf,
  type UsePdfResult,
} from './helpers.tsx';
export { generate, renderToBlob, renderToBuffer, renderToStream } from './browser-render.ts';
export type { TemplateLike } from './template-render.ts';
