// The invoice in @inkoxide/react (documents/invoice.tsx), with its logo read from disk.
// Run directly, by its path or through a symlink: writes the PDF. Imported (bench.tsx): only
// exports Invoice.
import { readFileSync, realpathSync } from 'node:fs';

import { renderToFile } from '@inkoxide/react';

import { Invoice as Document } from './documents/invoice.tsx';
import invoice from './invoice.json' with { type: 'json' };

const logo = readFileSync(new URL('./logo.jpg', import.meta.url));

export const Invoice = (inv: typeof invoice) => <Document logo={logo} {...inv} />;

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename)
  await renderToFile(<Invoice {...invoice} />, 'invoice.pdf');
