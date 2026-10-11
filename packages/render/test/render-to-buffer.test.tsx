/** A template is a component and its data are props: one import, one call, no registry. */
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable, Writable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

import { describe, expect, it } from 'vitest';

import {
  countPages,
  Document,
  Footer,
  Link,
  Page,
  PageBreak,
  PageNumber,
  RenderError,
  renderToBuffer,
  renderToFile,
  renderToStream,
  StyleSheet,
  Text,
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
    const path = join(mkdtempSync(join(tmpdir(), 'inkoxide-')), 'receipt.pdf');
    await renderToFile(<Receipt customer="Ada" lines={[]} />, path);
    expect(readFileSync(path).subarray(0, 5).toString('latin1')).toBe('%PDF-');
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
