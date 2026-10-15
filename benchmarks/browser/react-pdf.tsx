// @react-pdf/renderer's browser build, which renders on the page's main thread.
import * as rp from '@react-pdf/renderer';

import { Invoice } from '../documents/invoice-react-pdf.tsx';
import { type Kit, registerReactPdfFonts, Report } from '../documents/report.tsx';
import invoice from '../invoice.json';
import { measure } from './measure.ts';

registerReactPdfFonts(rp.Font, {
  regular: new URL('../../fixtures/fonts/Inter-Regular.ttf', import.meta.url).href,
  bold: new URL('../../fixtures/fonts/Inter-Bold.ttf', import.meta.url).href,
});
const logo = new URL('../logo.jpg', import.meta.url).href;

const render = async (el: Parameters<typeof rp.pdf>[0]) =>
  new Uint8Array(await (await rp.pdf(el).toBlob()).arrayBuffer());

await measure({
  invoice: () => render(<Invoice logo={logo} {...invoice} />),
  report: () => render(<Report kit={rp as unknown as Kit} />),
});
