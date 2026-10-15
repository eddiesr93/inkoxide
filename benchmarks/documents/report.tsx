// A 43-page report written once against the primitives both libraries share (Document, Page,
// View, Text and style objects in points), so both render the same content, font and layout. The
// server benchmark (bench.tsx) and the browser one (browser/) render it.
import type { ComponentType, ReactElement, ReactNode } from 'react';

export interface Kit {
  Document: ComponentType<{ children: ReactNode }>;
  Page: ComponentType<{ size: 'A4'; style: object; children: ReactNode }>;
  View: ComponentType<{ style?: object; children?: ReactNode; wrap?: boolean }>;
  Text: ComponentType<{ style?: object; children?: ReactNode }>;
}

/**
 * Inter for react-pdf, from `fonts` (paths in Node, URLs in a browser), and no hyphenation, which
 * Inkoxide does not apply by default either.
 */
export function registerReactPdfFonts(
  Font: { register(f: object): void; registerHyphenationCallback(cb: (w: string) => string[]): void },
  fonts: { regular: string; bold: string },
): void {
  Font.register({
    family: 'Inter',
    fonts: [{ src: fonts.regular }, { src: fonts.bold, fontWeight: 700 }],
  });
  Font.registerHyphenationCallback((word) => [word]);
}

const s = {
  page: { paddingTop: 48, paddingBottom: 48, paddingHorizontal: 48, fontSize: 10, fontFamily: 'Inter' },
  h1: { fontSize: 20, fontWeight: 700, marginBottom: 6 },
  h2: { fontSize: 13, fontWeight: 700, marginTop: 14, marginBottom: 6 },
  p: { marginBottom: 6, textAlign: 'justify' as const },
  row: {
    flexDirection: 'row' as const,
    borderBottomWidth: 0.5,
    borderBottomColor: '#d6d3ce',
    paddingVertical: 3,
  },
  head: { flexDirection: 'row' as const, backgroundColor: '#f4f3f0', paddingVertical: 4, fontWeight: 700 },
  name: { flexGrow: 1, flexBasis: 0 },
  num: { width: 70, textAlign: 'right' as const },
};

const WORDS =
  'quarterly revenue operations staffing procurement governance forecast inventory strategic partners efficiency portfolio reporting conversion infrastructure analysis compliance distribution'.split(
    ' ',
  );
const sentence = (i: number, n: number) =>
  Array.from({ length: n }, (_, k) => WORDS[(i * 7 + k * 3) % WORDS.length]).join(' ');

/** 40 sections, each with three paragraphs and a 25-row table: 43 pages in both libraries. */
export function Report({ kit }: { kit: Kit }): ReactElement {
  const { Document, Page, View, Text } = kit;
  return (
    <Document>
      <Page size="A4" style={s.page}>
        <Text style={s.h1}>Annual activity report</Text>
        {Array.from({ length: 40 }, (_, i) => (
          <View key={i}>
            <Text style={s.h2}>
              {i + 1}. {sentence(i, 4)}
            </Text>
            {[0, 1, 2].map((p) => (
              <Text key={p} style={s.p}>
                {sentence(i + p, 60)}.
              </Text>
            ))}
            <View style={s.head}>
              <Text style={s.name}>Item</Text>
              <Text style={s.num}>Quantity</Text>
              <Text style={s.num}>Amount</Text>
            </View>
            {Array.from({ length: 25 }, (_, r) => (
              <View key={r} style={s.row} wrap={false}>
                <Text style={s.name}>{sentence(i + r, 5)}</Text>
                <Text style={s.num}>{(r % 9) + 1}</Text>
                <Text style={s.num}>{((i + 1) * (r + 3) * 12.5).toFixed(2)}</Text>
              </View>
            ))}
          </View>
        ))}
      </Page>
    </Document>
  );
}
