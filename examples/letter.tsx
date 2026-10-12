import { Document, Footer, Page, renderToFile, Text, View } from '@inkoxide/render';

export interface LetterProps {
  date: string;
  to: string[];
  greeting: string;
  paragraphs: string[];
  sender: { name: string; title: string };
}

export default function Letter({ date, to, greeting, paragraphs, sender }: LetterProps) {
  return (
    <Document title="Letter" author={sender.name} lang="en-US">
      <Page size="Letter" className="px-24 pt-20 pb-24 text-[10.5pt] leading-relaxed text-slate-800">
        <Footer className="text-[7.5pt] text-slate-400">
          <View className="flex-row justify-between border-t border-slate-200 pt-3">
            <Text>Northwind Labs · 500 Market Street, San Francisco, CA 94105</Text>
            <Text href="mailto:hello@northwind.dev">hello@northwind.dev</Text>
          </View>
        </Footer>

        <View className="mb-14 flex-row items-center gap-2">
          <View className="h-3 w-3 rotate-45 bg-teal-600" />
          <Text className="text-base font-bold tracking-tight text-slate-900">Northwind Labs</Text>
        </View>

        <Text className="mb-8 text-slate-500">{date}</Text>
        <View className="mb-8">
          {to.map((line) => (
            <Text key={line}>{line}</Text>
          ))}
        </View>

        <Text className="mb-4">{greeting}</Text>
        {paragraphs.map((p) => (
          <Text key={p.slice(0, 24)} className="mb-3 text-justify" hyphenate>
            {p}
          </Text>
        ))}

        <View className="mt-6" break="avoid">
          <Text>Kind regards,</Text>
          <Text className="mt-10 font-bold text-slate-900">{sender.name}</Text>
          <Text className="text-slate-500">{sender.title}</Text>
        </View>
      </Page>
    </Document>
  );
}

export const sample: LetterProps = {
  date: 'October 11, 2026',
  to: ['Dr. Katherine Johnson', 'Langley Research Institute', '1 Discovery Way', 'Hampton, VA 23666'],
  greeting: 'Dear Dr. Johnson,',
  paragraphs: [
    'Thank you for meeting with our team last week. We were delighted by your interest in using our trajectory toolkit for the institute’s upcoming programme, and we have prepared the proposal you asked for, together with an estimate of the integration work.',
    'In summary, we would provide the analysis libraries under an institutional licence, train two members of your staff over the first month, and remain available for consultation throughout the project. The enclosed schedule shows a first milestone eight weeks after signature, with weekly progress reports in between.',
    'We understand that verification matters more than speed in this work. Every calculation the toolkit performs can be exported and checked independently, and we would welcome your team reviewing the methods before any results are relied upon.',
    'Please let us know if you would like to discuss the details in person. We look forward to working with you.',
  ],
  sender: { name: 'Margaret Hamilton', title: 'Director of Engineering' },
};

if (process.argv[1] === import.meta.filename) await renderToFile(<Letter {...sample} />, 'letter.pdf');
