// The invoice in @react-pdf/renderer (documents/invoice-react-pdf.tsx), with its logo on disk. Run
// directly, by its path or through a symlink: writes the PDF, with the font setup bench.tsx uses.
// Imported (bench.tsx): only exports Invoice.
import { realpathSync } from 'node:fs';
import { join } from 'node:path';

import { Font, renderToFile } from '@react-pdf/renderer';

import { Invoice as Document } from './documents/invoice-react-pdf.tsx';
import invoice from './invoice.json' with { type: 'json' };
import { setUpReactPdf } from './report.tsx';

const logo = join(import.meta.dirname, 'logo.jpg');

export const Invoice = (inv: typeof invoice) => <Document logo={logo} {...inv} />;

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  setUpReactPdf(Font);
  await renderToFile(<Invoice {...invoice} />, 'invoice-react-pdf.pdf');
}
