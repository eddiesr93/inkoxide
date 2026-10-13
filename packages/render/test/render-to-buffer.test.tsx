/** A template is a component and its data are props: one import, one call, no registry. */
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable, Writable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

import { afterAll, describe, expect, it } from 'vitest';

import {
  countPages,
  Document,
  FieldSet,
  Footer,
  Link,
  List,
  Page,
  PageBreak,
  PageNumber,
  RenderError,
  type RenderToBufferOptions,
  renderToBuffer,
  renderToFile,
  renderToStream,
  StyleSheet,
  Text,
  TextField,
  TotalPages,
  View,
} from '../src/index.ts';

const styles = StyleSheet.create({
  page: { padding: 48, fontSize: 11 },
  title: { fontSize: 20, fontWeight: 700, marginBottom: 12 },
  row: { flexDirection: 'row', justifyContent: 'space-between', paddingVertical: 4 },
  muted: { color: '#6b645d' },
});

interface Line {
  name: string;
  amount: number;
}

function Receipt({ customer, lines }: { customer: string; lines: Line[] }) {
  return (
    <Document title={`Receipt for ${customer}`}>
      <Page size="A4" style={styles.page}>
        <Text style={styles.title}>Receipt</Text>
        <Text style={[styles.muted, { marginBottom: 16 }]}>Billed to {customer}</Text>
        {lines.map((l, i) => (
          <View key={l.name} style={[styles.row, i % 2 === 1 && { backgroundColor: '#f4f3f0' }]}>
            <Text>{l.name}</Text>
            <Text>{l.amount.toFixed(2)}</Text>
          </View>
        ))}
        <Footer style={{ paddingHorizontal: 48, paddingBottom: 20 }}>
          <Text style={styles.muted}>
            Page <PageNumber /> of <TotalPages />
          </Text>
        </Footer>
      </Page>
    </Document>
  );
}

/** A new folder in the system's temp folder, removed after the tests. */
const dirs: string[] = [];
const tempDir = (): string => {
  const dir = mkdtempSync(join(tmpdir(), 'inkoxide-'));
  dirs.push(dir);
  return dir;
};
afterAll(() => {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
});

describe('renderToBuffer', () => {
  it('renders a component with props to a PDF', async () => {
    const pdf = await renderToBuffer(<Receipt customer="Ada" lines={[{ name: 'Coffee', amount: 3.5 }]} />);
    expect(Buffer.from(pdf.subarray(0, 5)).toString('latin1')).toBe('%PDF-');
    expect(countPages(pdf)).toBe(1);
  });

  it('paginates long content and fills in the page count', async () => {
    const lines = Array.from({ length: 200 }, (_, i) => ({ name: `Item ${i + 1}`, amount: i * 1.25 }));
    const pdf = await renderToBuffer(<Receipt customer="Ada" lines={lines} />);
    expect(countPages(pdf)).toBeGreaterThan(3);
  });

  it('writes a file', async () => {
    const path = join(tempDir(), 'receipt.pdf');
    await renderToFile(<Receipt customer="Ada" lines={[]} />, path);
    expect(readFileSync(path).subarray(0, 5).toString('latin1')).toBe('%PDF-');
  });

  it('fails with a TEMPLATE_ERROR for null, a string or a number in place of an element', async () => {
    const cases: Array<[unknown, string]> = [
      [null, 'A template must render exactly one <Document> at the top.'],
      [undefined, 'A template must render exactly one <Document> at the top.'],
      [
        'hello',
        'Text "hello" cannot be directly inside the root; render a <Document> with a <Page>, and put it in the page.',
      ],
      [
        42,
        'Text "42" cannot be directly inside the root; render a <Document> with a <Page>, and put it in the page.',
      ],
    ];
    for (const [element, message] of cases) {
      const err = await renderToBuffer(element as never).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({ code: 'TEMPLATE_ERROR', message });
    }
  });

  it('fails with a RenderError when the file cannot be written', async () => {
    const path = join(tempDir(), 'missing', 'receipt.pdf');
    const err = await renderToFile(<Receipt customer="Ada" lines={[]} />, path).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect(err).toMatchObject({ code: 'INTERNAL', cause: { code: 'ENOENT' } });
    expect((err as RenderError).message).toContain(`Cannot write the PDF to ${path}`);
  });

  it('writes a link annotation for an inline and a block Link', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page>
          <Text id="top">
            See <Link href="https://example.com/inline">the website</Link>
          </Text>
          <Link src="#top">Back to the top</Link>
        </Page>
      </Document>,
    );
    const bytes = Buffer.from(pdf).toString('latin1');
    expect(bytes.match(/\/Subtype ?\/Link/g)).toHaveLength(2);
    expect(bytes).toContain('https://example.com/inline');
  });

  it('writes the fields of a FieldSet as a field tree', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page>
          <FieldSet name="address">
            <TextField name="street" />
            <TextField name="city" />
          </FieldSet>
        </Page>
      </Document>,
    );
    const bytes = Buffer.from(pdf).toString('latin1');
    expect(bytes).toContain('/T (address)');
    expect(bytes).toContain('/T (street)');
    expect(bytes).toContain('/T (city)');
    // Both fields have the group as their parent.
    const group = bytes.split('endobj').find((o) => o.includes('/T (address)')) ?? '';
    const ref = /(\d+) 0 obj/.exec(group)?.[1];
    expect(bytes.split(`/Parent ${ref} 0 R`)).toHaveLength(3);
    expect(bytes).not.toContain('address.street');
  });

  it('writes a list box, and rejects a value that is not one of its options', async () => {
    const list = (value: string[]) => (
      <Document>
        <Page>
          <List name="days" options={['Mon', 'Tue', 'Wed']} multiSelect value={value} />
        </Page>
      </Document>
    );
    const bytes = Buffer.from(await renderToBuffer(list(['Wed', 'Mon']))).toString('latin1');
    expect(bytes).toContain('/FT /Ch');
    expect(bytes).toContain('/V [(Wed) (Mon)]');
    expect(bytes).toContain('/I [0 2]');
    const err = await renderToBuffer(list(['Sun'])).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect((err as RenderError).message).toContain('is not one of `options`');
  });

  it('repeats a fixed element with its link on every page, and its bookmark once', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page style={{ padding: 48 }}>
          <View
            fixed
            id="stamp"
            bookmark="Stamp"
            href="https://example.com/stamp"
            style={{ position: 'absolute', bottom: -30, right: 0, width: 40, height: 10 }}
          />
          <Text bookmark="Body">one</Text>
          <PageBreak />
          <Text href="#stamp">two</Text>
          <PageBreak />
          <Text>three</Text>
        </Page>
      </Document>,
      { compress: false },
    );
    expect(countPages(pdf)).toBe(3);
    const bytes = Buffer.from(pdf).toString('latin1');
    // One link per page from the fixed element, and the one to it on page 2.
    expect(bytes.match(/https:\/\/example\.com\/stamp/g)).toHaveLength(3);
    expect(bytes.match(/\/Subtype ?\/Link/g)).toHaveLength(4);
    expect(bytes.match(/\/Title ?\((Stamp|Body)\)/g)).toEqual(['/Title (Stamp)', '/Title (Body)']);
  });

  it('adds no blank page for a page break with nothing after it', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page size="A6" style={{ padding: 24 }}>
          <Text>1</Text>
          <View>
            <PageBreak />
          </View>
          <Text>2</Text>
          <View>
            <PageBreak />
          </View>
          <Footer>
            <Text>
              <PageNumber />
            </Text>
          </Footer>
        </Page>
      </Document>,
    );
    expect(countPages(pdf)).toBe(2);
  });

  it('renders the fixed watermark, border and page numbers of the guide', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page size="A4" style={{ padding: 48 }}>
          <Text
            fixed
            style={{ position: 'absolute', top: '45%', left: 0, right: 0, textAlign: 'center' }}
            className="rotate-[-30deg] text-[64pt] font-bold text-red-500/15"
          >
            DRAFT
          </Text>
          <View
            fixed
            style={{ position: 'absolute', top: -16, right: -16, bottom: -16, left: -16 }}
            className="border border-gray-300"
          />
          <Text
            fixed
            pages="notFirst"
            style={{ position: 'absolute', bottom: -36, left: 0, right: 0, textAlign: 'center' }}
            render={({ pageNumber, totalPages }) => `${pageNumber} / ${totalPages}`}
          />
          {Array.from({ length: 120 }, (_, i) => (
            <Text key={i}>Line {i + 1}</Text>
          ))}
        </Page>
      </Document>,
    );
    expect(countPages(pdf)).toBe(3);
  });

  it('draws debug outlines without changing the pages, the links or the fonts', async () => {
    const doc = (debug: boolean) => (
      <Document debug={debug}>
        <Page style={{ padding: 40 }}>
          <View style={{ margin: 8, padding: 6, borderWidth: 2 }}>
            <Text>
              See <Link href="https://example.com/debug">the website</Link>
            </Text>
          </View>
        </Page>
      </Document>
    );
    const plain = await renderToBuffer(doc(false));
    const debug = await renderToBuffer(doc(true));
    expect(Buffer.compare(debug, plain)).not.toBe(0);
    expect(countPages(debug)).toBe(countPages(plain));
    const text = (pdf: Uint8Array) => Buffer.from(pdf).toString('latin1');
    expect(text(debug).match(/\/Subtype ?\/Link/g)).toHaveLength(1);
    // Labels are drawn with lines: no font is added.
    const fonts = (pdf: Uint8Array) => text(pdf).match(/\/Type ?\/Font\b/g)?.length;
    expect(fonts(debug)).toBe(fonts(plain));
  });

  it('names the component when the structure is wrong', async () => {
    const Broken = () => (
      <Document>
        <Page>
          <Text>
            {/* A View inside a Text is not allowed. */}
            <View />
          </Text>
        </Page>
      </Document>
    );
    await expect(renderToBuffer(<Broken />)).rejects.toBeInstanceOf(RenderError);
  });

  it('fails with a RenderError when a render prop throws on a later pass', async () => {
    // The first pass renders with one page; the throw comes from the rebuild with the real count.
    const Growing = () => (
      <Document>
        <Page>
          <Text
            render={({ totalPages }) => {
              if (totalPages > 1) throw new Error('only one page expected');
              return 'one page';
            }}
          />
          <PageBreak />
          <Text>Second page</Text>
        </Page>
      </Document>
    );
    const err = await renderToBuffer(<Growing />).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect(err).toMatchObject({ code: 'TEMPLATE_ERROR' });
    expect((err as RenderError).message).toContain('only one page expected');
  });

  it('rejects a bad render option with a 400 at its path', async () => {
    const cases: Array<[RenderToBufferOptions, string]> = [
      [{ creationDate: 'yesterday' }, 'options.creationDate'],
      [{ threads: 0 }, 'options.threads'],
      [{ threads: 1000 }, 'options.threads'],
      [{ threads: 1.5 }, 'options.threads'],
    ];
    for (const [opts, path] of cases) {
      const err = await renderToBuffer(<Receipt customer="Ada" lines={[]} />, opts).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({
        code: 'ENGINE_REJECTED',
        status: 400,
        path,
        engineCode: 'INVALID_ARGUMENT',
      });
      expect((err as RenderError).message).toContain(`(at ${path})`);
    }
    expect(countPages(await renderToBuffer(<Receipt customer="Ada" lines={[]} />, { threads: 256 }))).toBe(1);
  });
});

describe('renderToStream', () => {
  it('streams the same bytes as renderToBuffer, as Buffer chunks', async () => {
    const element = <Receipt customer="Ada" lines={[{ name: 'Coffee', amount: 3.5 }]} />;
    const stream = await renderToStream(element);
    expect(stream).toBeInstanceOf(Readable);
    const chunks: Buffer[] = [];
    await pipeline(
      stream,
      new Writable({
        write(chunk: Buffer, _encoding, done) {
          chunks.push(chunk);
          done();
        },
      }),
    );
    expect(chunks.every((c) => Buffer.isBuffer(c))).toBe(true);
    const pdf = Buffer.concat(chunks);
    expect(pdf.equals(Buffer.from(await renderToBuffer(element)))).toBe(true);
  });

  it('rejects before streaming when the render fails', async () => {
    const Broken = () => (
      <Document>
        <Page>
          <Text>
            <View />
          </Text>
        </Page>
      </Document>
    );
    await expect(renderToStream(<Broken />)).rejects.toBeInstanceOf(RenderError);
  });
});
