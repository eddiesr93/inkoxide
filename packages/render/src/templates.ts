// `@inkoxide/render/templates`: what templates served by id share in every front end: the check of
// their data, a render of a tree built before, and of a stored document (GUIDE.md, "Serving
// templates"). Each front end's `generate` and `renderTree` come with it (`@inkoxide/react/templates`).
// test/exports.test.ts pins the list.
// In Node: the native binding, and the bundled assets from disk.
import './node-host.ts';

export type { TreeResult } from '@inkoxide/tree';
export {
  type GenerateOptions,
  parseData,
  renderDocument,
  renderSettled,
  type TemplateLike,
} from './pipeline.ts';
export type { RenderOpts, RenderResult, Timings } from './types.ts';
