// `@inkoxide/render/templates`: templates served by id, with a zod schema for their data
// (GUIDE.md, "Serving templates"). test/exports.test.ts pins the list.
export type { TreeResult } from '@inkoxide/react';
export {
  type GenerateOptions,
  generate,
  parseData,
  renderDocument,
  renderSettled,
  renderTree,
  type TemplateLike,
} from './pipeline.ts';
export type { RenderOpts, RenderResult, Timings } from './types.ts';
