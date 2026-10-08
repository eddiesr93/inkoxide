# @inkoxide/schema

TypeScript types and the JSON Schema for Inkoxide documents, schema
version 1. The types are generated from `schema/document.v1.json` by
`scripts/schema/gen-ts.sh`; do not edit `index.d.ts` by hand. This
package is not published. Install it from the repository with
`pnpm add file:../inkoxide/npm-schema`.

## Types

```ts
import type { Document, Node, TextRun } from '@inkoxide/schema';

const doc: Document = {
  version: 1,
  fonts: [{ id: 'Inter-Regular', family: 'Inter' }],
  pages: [{ size: { width: 595.28, height: 841.89 },
            children: [{ type: 'text', runs: [{ text: 'Hello', font: 'Inter-Regular', size: 24 }] }] }],
};
```

Every `$defs` entry of the schema is exported under its own name:
`Style`, `TableNode`, `Color`, and so on. `Node` is a union of the node
types, discriminated by `type`.

Two rules cannot be expressed in TypeScript:

- A text run has exactly one of `text` or `placeholder`.
- An image has exactly one of `src` or `data`.

Both are optional in the types. The engine and ajv report a document
that breaks either rule.

## Client-side validation (ajv)

```ts
import Ajv2020 from 'ajv/dist/2020.js';
import { schema, enginePath } from '@inkoxide/schema';

const validate = new Ajv2020({ discriminator: true, strict: false, validateFormats: false }).compile(schema);
if (!validate(doc)) {
  for (const e of validate.errors!) console.log(enginePath(e), e.message);
}
```

Pass `discriminator: true`. Without it, ajv reports an error for every
node type that does not match, not only for the one the `type` field
names. `strict: false` lets ajv accept the `discriminator` annotation
alongside `properties`. `validateFormats: false` skips `date-time`; add
`ajv-formats` if you want ajv to check it.

`enginePath(error)` turns an ajv error into the engine's path syntax,
`pages[0].children[3].style.width`. Paths from ajv and from the engine
(`validate()` / `InkoxideError.path`) then compare as follows:

| Problem | ajv keyword | Same path as the engine? |
|---|---|---|
| Unknown property | `additionalProperties` | yes (`enginePath` appends the property, as the engine does) |
| Wrong type, bad enum value, bad pattern | `type`, `enum`, `const`, `pattern`, `oneOf` | yes |
| Number out of range | `minimum`, `maximum`, `exclusiveMinimum`, … | yes |
| Missing required field of an object (`size.height`) | `required` | yes |
| Missing required field of a node (`text` without `runs`) | `required` | no: the engine reports the node's path, ajv the field's |
| Unsupported `version` | `const` | yes (`version`) |
| `text` and `placeholder` both set | `not` / `oneOf` on the run | no: the engine reports `….placeholder`, ajv the run |
| Unknown or unregistered font or image, duplicate id, unknown anchor, table structure | (none) | engine only: these need the registered fonts and images, or cross-references |

The schema checks shape only. Always handle errors from the engine as
well: ajv lets you show problems early, but the engine has the final
word.

## Exports

- `SCHEMA_VERSION`: `1`.
- `schema`: the JSON Schema object. It is also available as
  `@inkoxide/schema/document.v1.json`.
- `enginePath(error)`: see above.
