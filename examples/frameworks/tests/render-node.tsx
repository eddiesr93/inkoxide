// Prints the SHA-256 of the invoice rendered in Node by @inkoxide/react, with the native engine:
// what every page must render too. The tests run it with tsx.
import { deepStrictEqual } from 'node:assert';
import { createHash } from 'node:crypto';

import { renderToBuffer } from '@inkoxide/react';
import { createElement } from 'react';

import Invoice, { sample as original } from '../../invoice.tsx';
import { OPTIONS } from '../src/hooks.ts';
import { sample } from '../src/sample.ts';

deepStrictEqual(sample, original, "src/sample.ts is not examples/invoice.tsx's sample");
const pdf = await renderToBuffer(createElement(Invoice, sample), OPTIONS);
process.stdout.write(createHash('sha256').update(pdf).digest('hex'));
