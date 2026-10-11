import { enginePath, schema } from '@inkoxide/schema';
import addFormatsModule from 'ajv-formats';
import { Ajv2020, type ErrorObject, type ValidateFunction } from 'ajv/dist/2020.js';

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

/** Keywords that only say a `oneOf` branch did not apply: the least telling errors. */
const BRANCH_KEYWORDS: Readonly<Record<string, number>> = { type: 1, const: 1, enum: 1, oneOf: 2 };

/** Depth of the value an error is about. */
const depth = (e: ErrorObject) => e.instancePath.split('/').length;

/**
 * A `pattern` error on a described definition (`Percent`) says what the pattern means
 * ("must be a percentage within ±10000%…") rather than quoting the regular expression.
 */
function message(e: ErrorObject): string {
  const def = /^#\/\$defs\/(\w+)\/pattern$/.exec(e.schemaPath)?.[1];
  const defs = schema.$defs as Record<string, { description?: string }>;
  const description = def === undefined ? undefined : defs[def]?.description;
  if (!description) return e.message ?? e.keyword;
  return `must be ${description[0]?.toLowerCase()}${description.slice(1).replace(/\.$|`/g, '')}`;
}

/** The value at a JSON Pointer of the document. */
function valueAt(doc: unknown, pointer: string): unknown {
  let v = doc;
  for (const key of pointer.split('/').slice(1)) {
    if (typeof v !== 'object' || v === null) return undefined;
    v = (v as Record<string, unknown>)[key.replace(/~1/g, '/').replace(/~0/g, '~')];
  }
  return v;
}

/** Definitions whose `pattern` describes strings with this suffix. */
const SUFFIXED: Readonly<Record<string, string>> = { Percent: '%', Fr: 'fr' };

/**
 * 0 for a `pattern` error from the definition the value was written for (`"2000000fr"` and
 * `Fr`), so it comes before one from a sibling definition (`Percent`); 1 otherwise.
 */
function suffixRank(e: ErrorObject, doc: unknown): number {
  const def = /^#\/\$defs\/(\w+)\/pattern$/.exec(e.schemaPath)?.[1];
  const suffix = def === undefined ? undefined : SUFFIXED[def];
  if (suffix === undefined) return 1;
  const value = valueAt(doc, e.instancePath);
  return typeof value === 'string' && value.trim().endsWith(suffix) ? 0 : 1;
}

/**
 * Validates a tree against the engine's JSON Schema with ajv (dev only): shape errors
 * show up with component stacks before the engine sees the document. Empty when valid.
 * The most telling problem comes first: for a value that matches no `oneOf` branch (a
 * length, a transform function), the error deepest in the value, and a bound or pattern
 * before "must be number" from a branch that does not apply; of two patterns, the one for
 * the value's suffix (`fr` or `%`).
 */
export function validateTree(doc: unknown): SchemaProblem[] {
  validator ??= compile();
  if (validator(doc)) return [];
  const errors = [...(validator.errors ?? [])].sort(
    (a, b) =>
      depth(b) - depth(a) ||
      (BRANCH_KEYWORDS[a.keyword] ?? 0) - (BRANCH_KEYWORDS[b.keyword] ?? 0) ||
      suffixRank(a, doc) - suffixRank(b, doc),
  );
  return errors.map((e) => ({
    path: enginePath(e),
    message: message(e),
    keyword: e.keyword,
  }));
}
