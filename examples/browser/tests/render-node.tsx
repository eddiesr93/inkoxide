// Prints the SHA-256 of the invoice rendered in Node, with the native engine: what the page must
// render too. The tests run it with tsx.
import { createHash } from 'node:crypto';

import { renderToBuffer } from '@inkoxide/render';
import { createElement } from 'react';

import Invoice, { OPTIONS, sample } from '../src/Invoice.tsx';

const pdf = await renderToBuffer(createElement(Invoice, sample), OPTIONS);
process.stdout.write(createHash('sha256').update(pdf).digest('hex'));
