/** A template is a component and its data are props: one import, one call, no registry. */
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable, Writable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

import { afterAll, describe, expect, it } from 'vitest';

import {
  Canvas,
  countPages,
  Defs,
  Document,
  type DocumentProps,
  FieldSet,
  Footer,
  Line,
  Link,
  List,
  Marker,
  Note,
  Page,
  PageBreak,
  PageNumber,
  Path,
  RenderError,
  type RenderToBufferOptions,
  renderToBuffer,
  renderToFile,
  renderToStream,
  StyleSheet,
  Svg,
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

  it('writes a formatted text field with its scripts, and leaves them out in PDF/A', async () => {
    const doc = (
      <Document>
        <Page>
          <TextField name="fee" value="1250" format={{ type: 'number', currency: '$' }} />
        </Page>
      </Document>
    );
    const bytes = Buffer.from(await renderToBuffer(doc)).toString('latin1');
    expect(bytes).toContain('/V (1250)');
    expect(bytes).toContain('AFNumber_Format(2, 0, 0, 0, "$", true);');
    const pdfA = Buffer.from(await renderToBuffer(doc, { pdfA: true })).toString('latin1');
    expect(pdfA).toContain('/V (1250)');
    expect(pdfA).not.toContain('AFNumber_Format');
  });

  it('writes a note as a text annotation with its icon', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page>
          <Note author="Ana" style={{ backgroundColor: '#3b82f6' }}>
            Check the totals
          </Note>
        </Page>
      </Document>,
    );
    const bytes = Buffer.from(pdf).toString('latin1');
    expect(bytes).toMatch(/\/Subtype ?\/Text/);
    expect(bytes).toContain('/Contents (Check the totals)');
    expect(bytes).toContain('/T (Ana)');
    expect(bytes).toMatch(/\/AP ?<</);
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

  it('draws markers, and Canvas clips and gradients', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page style={{ padding: 40 }}>
          <Svg width={120} height={40}>
            <Defs>
              <Marker id="a" viewBox="0 0 10 10" refX={10} refY={5} orient="auto-start-reverse">
                <Path d="M0 0L10 5L0 10Z" fill="context-stroke" />
              </Marker>
            </Defs>
            <Line
              x1={10}
              y1={20}
              x2={110}
              y2={20}
              stroke="#4f46e5"
              strokeWidth={2}
              markerStart="url(#a)"
              markerEnd="url(#a)"
            />
          </Svg>
          <Canvas
            width={120}
            height={60}
            paint={(p, w, h) => {
              const g = p.linearGradient(0, 0, w, 0).stop(0, '#4f46e5').stop(1, '#c7d2fe', 0.5);
              p.save().roundedRect(0, 0, w, h, 8).clip();
              p.rect(0, 0, w, h).fill(g);
              p.restore().miterLimit(4).lineWidth(3).polygon([10, 50], [60, 10], [110, 50]).stroke(g);
            }}
          />
        </Page>
      </Document>,
    );
    expect(countPages(pdf)).toBe(1);
    expect(Buffer.from(pdf).toString('latin1')).toMatch(/\/ShadingType ?2/);
  });

  it('renders superscripts, subscripts and font features', async () => {
    const doc = (features: boolean) => (
      <Document>
        <Page style={{ padding: 40 }}>
          <Text style={features ? { fontFeatureSettings: ['tnum', 'zero'] } : {}}>
            x<Text style={{ verticalAlign: 'super', fontSize: 7 }}>2</Text> H
            <Text style={{ verticalAlign: 'sub', fontSize: 7 }}>2</Text>O 1,110.10
          </Text>
        </Page>
      </Document>
    );
    const plain = await renderToBuffer(doc(false));
    const tnum = await renderToBuffer(doc(true));
    expect(countPages(plain)).toBe(1);
    // Other glyphs: tabular digits and a slashed zero.
    expect(Buffer.compare(tnum, plain)).not.toBe(0);
  });

  it('breaks the lines of a page with textWrap pretty over each paragraph', async () => {
    const words =
      'In olden times when wishing still helped one, there lived a king whose daughters were all beautiful, but the youngest was so beautiful that the sun itself was astonished.';
    const doc = (wrap: 'wrap' | 'pretty') => (
      <Document>
        <Page style={{ padding: 40, textWrap: wrap }}>
          <View style={{ width: 150 }}>
            <Text>{words}</Text>
          </View>
        </Page>
      </Document>
    );
    const wrap = await renderToBuffer(doc('wrap'));
    const pretty = await renderToBuffer(doc('pretty'));
    expect(Buffer.compare(pretty, wrap)).not.toBe(0);
  });

  it('wraps text around a float', async () => {
    const doc = (float: 'left' | 'none') => (
      <Document>
        <Page style={{ padding: 40 }}>
          <View style={{ float, width: 80, height: 60, backgroundColor: '#ddd' }} />
          <Text>
            In olden times when wishing still helped one, there lived a king whose daughters were all
            beautiful, but the youngest was so beautiful that the sun itself was astonished.
          </Text>
        </Page>
      </Document>
    );
    const floated = await renderToBuffer(doc('left'));
    expect(countPages(floated)).toBe(1);
    expect(Buffer.compare(floated, await renderToBuffer(doc('none')))).not.toBe(0);
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
      [{ pdfA: '2a' as '2b' }, 'options.pdfA'],
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

  it('writes the PDF/A level asked for', async () => {
    const part = async (pdfA: RenderToBufferOptions['pdfA']) => {
      const pdf = Buffer.from(await renderToBuffer(<Receipt customer="Ada" lines={[]} />, { pdfA })).toString(
        'latin1',
      );
      return /<pdfaid:part>(\d)<\/pdfaid:part>/.exec(pdf)?.[1];
    };
    expect(await part(undefined)).toBeUndefined();
    expect(await part(true)).toBe('2');
    expect(await part('2b')).toBe('2');
    expect(await part('3b')).toBe('3');
    expect(await part('1b')).toBe('1');
  });

  it('writes the PDF/A level of <Document conformance>, unless the pdfA option says otherwise', async () => {
    const part = async (conformance: DocumentProps['conformance'], opts: RenderToBufferOptions = {}) => {
      const doc = (
        <Document conformance={conformance}>
          <Page>
            <Text>Archived</Text>
          </Page>
        </Document>
      );
      const pdf = Buffer.from(await renderToBuffer(doc, opts)).toString('latin1');
      return /<pdfaid:part>(\d)<\/pdfaid:part>/.exec(pdf)?.[1];
    };
    expect(await part('PDF/A-1b')).toBe('1');
    expect(await part('PDF/A-3')).toBe('3');
    expect(await part('PDF/A-1b', { pdfA: '2b' })).toBe('2');
    expect(await part('PDF/A-1b', { pdfA: false })).toBeUndefined();
    expect(await part(undefined, { pdfA: true })).toBe('2');
  });

  it('encrypts with the Document passwords, unless the encryption option replaces them', async () => {
    const secret = 's3cret-marker';
    const doc = (props: Partial<DocumentProps>) => (
      <Document title="Locked" {...props}>
        <Page>
          <Text>Private</Text>
        </Page>
      </Document>
    );
    const text = async (element: ReturnType<typeof doc>, opts: RenderToBufferOptions = {}) =>
      Buffer.from(await renderToBuffer(element, opts)).toString('latin1');
    const props = {
      userPassword: secret,
      ownerPassword: 'owner',
      permissions: { printing: 'lowResolution' as const },
    };
    const locked = await text(doc(props));
    expect(locked).toMatch(/\/Encrypt \d+ 0 R/);
    expect(locked).toMatch(/\/P -3388\b/);
    expect(locked).not.toContain(secret);
    expect(locked).not.toContain('Locked');
    expect(await text(doc(props))).toBe(locked);
    // The option replaces the props as a whole: no permissions now.
    const replaced = await text(doc(props), { encryption: { userPassword: 'other' } });
    expect(replaced).toMatch(/\/P -3392\b/);
    expect(await text(doc({}))).not.toMatch(/\/Encrypt/);
    // A bad value names its path and never the password.
    for (const [p, opts, path] of [
      [{ userPassword: secret, permissions: {} }, {}, 'options.encryption.permissions'],
      [{ userPassword: `${secret}é` }, {}, 'options.encryption.userPassword'],
      [{}, { encryption: { userPassword: secret }, pdfA: true }, 'options.encryption'],
    ] as const) {
      const err = await renderToBuffer(doc(p), opts).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({ code: 'ENGINE_REJECTED', engineCode: 'INVALID_ARGUMENT', path });
      expect(JSON.stringify(err)).not.toContain(secret);
      expect((err as Error).message).not.toContain(secret);
    }
  });

  it('names the component PDF/A-1b rejects for its transparency', async () => {
    function Faded() {
      return <View style={{ opacity: 0.5, height: 10 }} />;
    }
    const doc = (
      <Document>
        <Page>
          <Faded />
        </Page>
      </Document>
    );
    expect(countPages(await renderToBuffer(doc, { pdfA: '2b' }))).toBe(1);
    const err = await renderToBuffer(doc, { pdfA: '1b' }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect(err).toMatchObject({
      code: 'ENGINE_REJECTED',
      engineCode: 'PDFA_CONFLICT',
      path: 'pages[0].children[0].style.opacity',
      component: 'View',
    });
    expect((err as RenderError).componentStack).toContain('Faded');
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
