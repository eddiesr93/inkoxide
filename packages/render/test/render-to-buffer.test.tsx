/** A template is a component and its data are props: one import, one call, no registry. */
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  countPages,
  Document,
  Footer,
  Link,
  Page,
  PageNumber,
  RenderError,
  renderToBuffer,
  renderToFile,
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
