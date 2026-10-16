// Prints the SHA-256 of the invoice and of the letter rendered in Node, with the native engine,
// as JSON: what the page must render too. The tests run it with tsx.
import { createHash } from 'node:crypto';

import { renderToBuffer } from '@inkoxide/render';
import { createElement } from 'react';

import Invoice, { OPTIONS, sample } from '../src/Invoice.tsx';
import Letter from '../src/Letter.tsx';

const sha256 = (pdf: Uint8Array) => createHash('sha256').update(pdf).digest('hex');
const invoice = sha256(await renderToBuffer(createElement(Invoice, sample), OPTIONS));
const letter = sha256(await renderToBuffer(createElement(Letter), OPTIONS));
process.stdout.write(JSON.stringify({ invoice, letter }));
