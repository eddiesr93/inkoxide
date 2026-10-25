import { describe, expect, it } from 'vitest';

/**
 * Attachments: files a PDF carries, from `<Document attachments>` or, for a JSON document, the
 * render option `attachments`; and an e-invoice's metadata.
 */
import { renderTemplate } from '../src/index.ts';
import {
  Attachment,
  Document,
  facturX,
  facturXRelationship,
  Page,
  RenderError,
  renderToBuffer,
  Text,
} from '../src/index.ts';
import { renderDocument } from '../src/templates.ts';

const XML = '<?xml version="1.0"?><Invoice><Total>9.70</Total></Invoice>';
const latin1 = (pdf: Uint8Array) => Buffer.from(pdf).toString('latin1');

const Invoice = ({ conformance }: { conformance?: 'PDF/A-3b' | 'PDF/A-2b' }) => (
  <Document
    title="Invoice"
    conformance={conformance}
    attachments={[
      {
        name: 'factur-x.xml',
        data: XML,
        mimeType: 'text/xml',
        relationship: facturXRelationship('EN 16931'),
        description: 'Invoice data',
      },
    ]}
    xmpExtensions={conformance ? [facturX({ profile: 'EN 16931' })] : undefined}
  >
    <Page>
      <Attachment file="factur-x.xml" icon="paperclip" />
      <Text>Total: 9.70</Text>
    </Page>
  </Document>
);

describe('attachments', () => {
  it('make a PDF/A-3b e-invoice with its XML and metadata', async () => {
    const pdf = latin1(await renderToBuffer(<Invoice conformance="PDF/A-3b" />, { compress: false }));
    expect(pdf).toContain('/EmbeddedFiles');
    expect(pdf).toContain('/AFRelationship /Alternative');
    expect(pdf).toContain('<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>');
    expect(pdf).toContain(XML);
  });

  it('are refused at PDF/A-2b', async () => {
    const e = await renderToBuffer(<Invoice conformance="PDF/A-2b" />).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(RenderError);
    expect((e as RenderError).path).toBe('attachments[0]');
  });

  it('come from the render option attachments for a JSON document', async () => {
    const { document } = renderTemplate(<Invoice />);
    const [file] = document.attachments ?? [];
    const doc = { ...document, attachments: [{ name: file?.name ?? '', src: 'xml' }] };
    const { pdf } = await renderDocument(
      doc,
      {},
      {
        compress: false,
        attachments: { xml: new TextEncoder().encode(XML) },
      },
    );
    expect(latin1(pdf)).toContain(XML);
    const missing = await renderDocument(doc).catch((x: unknown) => x);
    expect((missing as RenderError).message).toContain('attachment `xml` was not provided');
  });
});
