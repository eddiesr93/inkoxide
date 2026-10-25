// A 43-page report: 40 sections of headings, justified paragraphs and tables, in Inter. The server
// benchmark (bench.tsx) and the browser one (browser/) render it.
import { Document, Page, Text, View } from '@inkoxide/react';
import type { ReactElement } from 'react';

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

/** 40 sections, each with three paragraphs and a 25-row table: 43 pages. */
export function Report(): ReactElement {
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
