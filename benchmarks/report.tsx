// The report (documents/report.tsx), with react-pdf's fonts read from fixtures/fonts.
import { join } from 'node:path';

import { registerReactPdfFonts } from './documents/report.tsx';

export { type Kit, Report } from './documents/report.tsx';

/** Inter for react-pdf, from fixtures/fonts, and no hyphenation. */
export function setUpReactPdf(Font: Parameters<typeof registerReactPdfFonts>[0]): void {
  const fonts = join(import.meta.dirname, '..', 'fixtures', 'fonts');
  registerReactPdfFonts(Font, {
    regular: join(fonts, 'Inter-Regular.ttf'),
    bold: join(fonts, 'Inter-Bold.ttf'),
  });
}
