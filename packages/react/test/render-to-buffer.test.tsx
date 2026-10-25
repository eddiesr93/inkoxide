/** A template is a component and its data are props: one import, one call, no registry. */
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable, Writable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { inflateSync } from 'node:zlib';

import { afterAll, describe, expect, it } from 'vitest';

import {
  Canvas,
  countPages,
  Defs,
  Document,
  type DocumentProps,
  FieldSet,
  Image,
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
  TextPath,
  Tspan,
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
              p.font('Inter-Bold', 14)
                .fillColor('white')
                .text('Canvas', 0, 20, { width: w, align: 'center' });
            }}
          />
        </Page>
      </Document>,
    );
    expect(countPages(pdf)).toBe(1);
    expect(Buffer.from(pdf).toString('latin1')).toMatch(/\/ShadingType ?2/);
  });

  it('draws SVG text as PDF text with the faces it uses', async () => {
    const doc = (text: boolean) => (
      <Document>
        <Page style={{ padding: 40 }}>
          <Text>Page text</Text>
          <Svg width={120} height={40}>
            {text && (
              <Text x={60} y={25} textAnchor="middle" fontWeight="bold" fill="#4f46e5">
                Chart <Tspan fill="#c0392b">title</Tspan>
              </Text>
            )}
          </Svg>
        </Page>
      </Document>
    );
    const fonts = (pdf: Uint8Array) =>
      Buffer.from(pdf)
        .toString('latin1')
        .match(/\/FontFile2/g)?.length;
    // Inter Bold, used only in the drawing, is embedded too.
    expect(fonts(await renderToBuffer(doc(true)))).toBe(2);
    expect(fonts(await renderToBuffer(doc(false)))).toBe(1);
  });

  it('lays one component out by the size of each page it is on', async () => {
    // Taller on a page narrower than 150mm: two of them fill more than an A5 page.
    const Block = () => <View style={{ height: 100, '@media (width < 150mm)': { height: 400 } }} />;
    const doc = (narrow: boolean) => (
      <Document>
        <Page size="A4">
          <Block />
          <Block />
        </Page>
        <Page size={narrow ? 'A5' : 'A4'}>
          <Block />
          <Block />
        </Page>
      </Document>
    );
    expect(countPages(await renderToBuffer(doc(false)))).toBe(2);
    expect(countPages(await renderToBuffer(doc(true)))).toBe(3);
  });

  it('draws text on a path and vertical text from an Svg', async () => {
    const pdf = await renderToBuffer(
      <Document>
        <Page style={{ padding: 40 }}>
          <Svg width={200} height={120}>
            <Defs>
              <Path id="curve" d="M10 100 Q 100 0 190 100" />
            </Defs>
            <Text fontSize={12}>
              <TextPath href="#curve" startOffset="50%" textAnchor="middle">
                Along the curve
              </TextPath>
            </Text>
            <Text x={20} y={10} fontSize={10} writingMode="vertical-rl">
              Down
            </Text>
          </Svg>
        </Page>
      </Document>,
      { compress: false },
    );
    // The drawing is a form, compressed whatever the option: its stream
    // inflated.
    const raw = Buffer.from(pdf).toString('latin1');
    const forms = [...raw.matchAll(/FlateDecode[^]*?stream\r?\n/g)].map((m) => {
      const start = (m.index ?? 0) + m[0].length;
      const bytes = Buffer.from(raw.slice(start, raw.indexOf('endstream', start)), 'latin1');
      try {
        return inflateSync(bytes).toString('latin1');
      } catch {
        return '';
      }
    });
    const text = forms.join('');
    // Each glyph on the curve is turned: a text matrix of its own.
    expect(text.match(/ Tm/g)?.length ?? 0).toBeGreaterThan(10);
    // Extraction reads it in order.
    expect(text).toContain('/ActualText');
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
      [{ pdfA: '2u' as '2b' }, 'options.pdfA'],
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

  it('rejects a render option of the wrong type with a 400 at its path, in no component', async () => {
    const cases: Array<[Record<string, unknown>, string]> = [
      [{ threads: '4' }, 'options.threads'],
      [{ pdfA: 5 }, 'options.pdfA'],
      [{ compress: 'no' }, 'options.compress'],
      [{ subsetFonts: 1 }, 'options.subsetFonts'],
      [{ creationDate: 20260101 }, 'options.creationDate'],
      [{ images: { logo: 'logo.png' } }, 'options.images.logo'],
      [
        { encryption: { userPassword: 'u', ownerPassword: 'o', permissions: { copying: 'yes' } } },
        'options.encryption.permissions.copying',
      ],
      [{ localImages: {} }, 'options.localImages.root'],
      [{ localImages: { root: [1] } }, 'options.localImages.root'],
      [{ remoteImages: { allow: 'cdn.example.com' } }, 'options.remoteImages.allow'],
    ];
    for (const [opts, path] of cases) {
      const err = await renderToBuffer(<Receipt customer="Ada" lines={[]} />, opts).catch((e: unknown) => e);
      expect(err, JSON.stringify(opts)).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({ code: 'ENGINE_REJECTED', status: 400, path, component: undefined });
      expect((err as RenderError).message).toContain(path.slice('options.'.length));
      expect((err as RenderError).message).not.toContain('<Document>');
    }
    // null is an option left out.
    const nulls = {
      threads: null,
      compress: null,
      pdfA: null,
      images: null,
      attachments: null,
    } as unknown as RenderToBufferOptions;
    expect(countPages(await renderToBuffer(<Receipt customer="Ada" lines={[]} />, nulls))).toBe(1);
    // A <Document> permission of the wrong type, from JavaScript, is the same.
    const props = { userPassword: 'u', ownerPassword: 'o', permissions: { copying: 'yes' } };
    const fromProps = await renderToBuffer(
      <Document {...(props as unknown as DocumentProps)}>
        <Page>
          <Text>Locked</Text>
        </Page>
      </Document>,
    ).catch((e: unknown) => e);
    expect(fromProps).toMatchObject({
      code: 'ENGINE_REJECTED',
      path: 'options.encryption.permissions.copying',
      component: undefined,
    });
  });

  it('names no component for a render option the engine refuses', async () => {
    const err = await renderToBuffer(<Receipt customer="Ada" lines={[]} />, { threads: 0 }).catch(
      (e: unknown) => e,
    );
    expect(err).toMatchObject({ code: 'ENGINE_REJECTED', path: 'options.threads', component: undefined });
    expect((err as RenderError).message).not.toMatch(/ in </);
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
      [
        { userPassword: secret, ownerPassword: secret, permissions: {} },
        {},
        'options.encryption.permissions',
      ],
      [{ userPassword: `${secret}\u{E000}` }, {}, 'options.encryption.userPassword'],
      [{ userPassword: secret, entropy: new Uint8Array(4) }, {}, 'options.encryption.entropy'],
      [{}, { encryption: { userPassword: secret }, pdfA: true }, 'options.encryption'],
    ] as const) {
      const err = await renderToBuffer(doc(p), opts).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(RenderError);
      expect(err).toMatchObject({ code: 'ENGINE_REJECTED', engineCode: 'INVALID_ARGUMENT', path });
      expect(JSON.stringify(err)).not.toContain(secret);
      expect((err as Error).message).not.toContain(secret);
    }
    // A password of another type is refused before the engine converts it, without its value.
    const typed = await renderToBuffer(doc({}), {
      encryption: {
        userPassword: 1234 as unknown as string,
        ownerPassword: { secret } as unknown as string,
      },
    }).catch((e: unknown) => e);
    expect(typed).toMatchObject({
      code: 'ENGINE_REJECTED',
      path: 'options.encryption.userPassword',
    });
    expect(JSON.stringify(typed)).not.toMatch(/1234|s3cret/);
    expect((typed as Error).message).not.toMatch(/1234|s3cret/);
    const badEntropy = await renderToBuffer(doc({}), {
      encryption: { userPassword: 'u', entropy: secret as unknown as boolean },
    }).catch((e: unknown) => e);
    expect(badEntropy).toMatchObject({ code: 'ENGINE_REJECTED', path: 'options.encryption.entropy' });
    expect((badEntropy as Error).message).not.toContain(secret);
    // Unicode passwords; with entropy, each render gets other keys.
    expect(await text(doc({ userPassword: 'parolă' }))).toMatch(/\/R 6/);
    const random = { userPassword: 'u', entropy: true };
    expect(await text(doc(random))).not.toBe(await text(doc(random)));
    const fixed = { userPassword: 'u', entropy: new Uint8Array(32).fill(9) };
    expect(await text(doc(fixed))).toBe(await text(doc(fixed)));
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

  it('draws partial alpha on white at PDF/A-1b with <Document flattenAlpha> or the option', async () => {
    const logo = readFileSync(new URL('../../../fixtures/images/logo.png', import.meta.url));
    const doc = (flattenAlpha?: boolean) => (
      <Document conformance="PDF/A-1b" flattenAlpha={flattenAlpha}>
        <Page>
          <Image src="logo" style={{ width: 20 }} />
        </Page>
      </Document>
    );
    const opts = { images: { logo } };
    const refused = await renderToBuffer(doc(), opts).catch((e: unknown) => e);
    expect(refused).toMatchObject({ engineCode: 'PDFA_CONFLICT', path: 'pages[0].children[0].src' });
    expect((refused as Error).message).toContain('flattenAlpha');
    for (const pdf of [
      await renderToBuffer(doc(true), opts),
      await renderToBuffer(doc(), { ...opts, flattenAlpha: true }),
    ]) {
      expect(Buffer.from(pdf).toString('latin1')).not.toContain('/SMask');
    }
    const kept = renderToBuffer(doc(true), { ...opts, flattenAlpha: false });
    await expect(kept).rejects.toMatchObject({ engineCode: 'PDFA_CONFLICT' });
  });

  it('tags as <Document tagged pdfUA> say, unless the tagged and pdfUA options say otherwise', async () => {
    const pdf = async (props: Partial<DocumentProps>, opts: RenderToBufferOptions = {}) =>
      Buffer.from(
        await renderToBuffer(
          <Document title="Tagged" lang="en" {...props}>
            <Page>
              <Text>Read</Text>
            </Page>
          </Document>,
          opts,
        ),
      ).toString('latin1');
    const tagged = (s: string) => s.includes('/StructTreeRoot');
    const ua = (s: string) => s.includes('<pdfuaid:part>1</pdfuaid:part>');
    expect(tagged(await pdf({}))).toBe(false);
    expect(tagged(await pdf({ tagged: true }))).toBe(true);
    expect(tagged(await pdf({ tagged: true }, { tagged: false }))).toBe(false);
    expect(tagged(await pdf({}, { tagged: true }))).toBe(true);
    expect(ua(await pdf({ pdfUA: true }))).toBe(true);
    expect(ua(await pdf({ pdfUA: true }, { pdfUA: false }))).toBe(false);
    const both = await pdf({}, { pdfUA: true });
    expect([tagged(both), ua(both)]).toEqual([true, true]);
    const two = await pdf({ pdfUA: '2' });
    expect([two.startsWith('%PDF-2.0'), two.includes('<pdfuaid:part>2</pdfuaid:part>')]).toEqual([
      true,
      true,
    ]);
    expect((await pdf({ pdfUA: '2' }, { pdfUA: '1' })).startsWith('%PDF-1.7')).toBe(true);
    const bad = await renderToBuffer(<Document />, { pdfUA: 3 as unknown as '1' }).catch((e: unknown) => e);
    expect(bad).toMatchObject({ code: 'ENGINE_REJECTED', path: 'options.pdfUA' });
  });

  it('names the component PDF/UA-1 rejects for what it lacks', async () => {
    function Street() {
      return <TextField name="street" />;
    }
    const doc = (
      <Document title="Report" lang="en">
        <Page>
          <Street />
        </Page>
      </Document>
    );
    expect(countPages(await renderToBuffer(doc))).toBe(1);
    const err = await renderToBuffer(doc, { pdfUA: true }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RenderError);
    expect(err).toMatchObject({
      code: 'ENGINE_REJECTED',
      engineCode: 'UA_CONFLICT',
      path: 'pages[0].children[0].field.label',
      component: 'Field',
    });
    expect((err as RenderError).componentStack).toContain('Street');
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
