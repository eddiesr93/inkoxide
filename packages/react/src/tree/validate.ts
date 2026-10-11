import { enginePath, schema } from '@inkoxide/schema';
import addFormatsModule from 'ajv-formats';
import { Ajv2020, type ValidateFunction } from 'ajv/dist/2020.js';

export interface SchemaProblem {
  path: string;
  message: string;
  keyword: string;
}

let validator: ValidateFunction | undefined;

// ajv-formats is CommonJS with a default export; normalise the interop shape.
const addFormats = ((addFormatsModule as unknown as { default?: unknown }).default ?? addFormatsModule) as (
  ajv: Ajv2020,
) => Ajv2020;

function compile(): ValidateFunction {
  const ajv = new Ajv2020({ discriminator: true, strict: false, allErrors: false });
  addFormats(ajv);
  return ajv.compile(schema);
}

/**
 * Validates a tree against the engine's JSON Schema with ajv (dev only): shape errors
 * show up with component stacks before the engine sees the document. Empty when valid.
 */
export function validateTree(doc: unknown): SchemaProblem[] {
  validator ??= compile();
  if (validator(doc)) return [];
  return (validator.errors ?? []).map((e) => ({
    path: enginePath(e),
    message: e.message ?? e.keyword,
    keyword: e.keyword,
  }));
}
