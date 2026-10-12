import { Circle, Document, Page, Path, renderToFile, Svg, Text, View } from '@inkoxide/render';

export interface CertificateProps {
  recipient: string;
  course: string;
  hours: number;
  date: string;
  id: string;
}

/** A rosette seal drawn with SVG. */
function Seal() {
  const points = Array.from({ length: 24 }, (_, i) => {
    const a = (i * Math.PI) / 12;
    const r = i % 2 === 0 ? 46 : 40;
    return `${50 + r * Math.cos(a)},${50 + r * Math.sin(a)}`;
  });
  return (
    <Svg width={84} height={84} viewBox="0 0 100 100">
      <Path d={`M${points.join(' L')} Z`} fill="#b45309" />
      <Circle cx={50} cy={50} r={34} fill="#f59e0b" />
      <Circle cx={50} cy={50} r={28} fill="none" stroke="#fef3c7" strokeWidth={1.5} />
      <Path
        d="M38 51 L46 59 L63 41"
        fill="none"
        stroke="white"
        strokeWidth={5}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </Svg>
  );
}

function Signature({ name, caption }: { name: string; caption: string }) {
  return (
    <View className="w-48 items-center">
      <View className="mb-2 w-full border-b border-stone-400" />
      <Text className="font-bold">{name}</Text>
      <Text className="text-stone-500">{caption}</Text>
    </View>
  );
}

export default function Certificate({ recipient, course, hours, date, id }: CertificateProps) {
  return (
    <Document title={`Certificate: ${recipient}`}>
      <Page size="A4-landscape" className="bg-amber-50 p-6 text-[9pt] text-stone-800">
        <View className="flex-1 border-2 border-amber-700 p-1.5">
          <View className="flex-1 items-center border border-amber-700/50 px-20 py-12">
            <Text className="text-[8pt] font-bold uppercase tracking-[0.4em] text-amber-700">
              Acme Academy
            </Text>
            <View className="flex-1 items-center justify-center">
              <Text className="font-display text-[40pt] text-stone-900">Certificate of Completion</Text>
              <Text className="mt-4 text-sm text-stone-500">This certifies that</Text>
              <Text className="mt-4 border-b border-amber-700/40 px-10 pb-2 font-display text-[30pt] text-amber-800">
                {recipient}
              </Text>
              <Text className="mt-5 max-w-md text-center text-sm leading-relaxed text-stone-600">
                has completed <Text className="font-bold text-stone-900">{course}</Text>, a {hours}-hour
                course with practical assessments, and is awarded this certificate.
              </Text>
            </View>
            <View className="w-full flex-row items-end justify-between">
              <Signature name="Grace Hopper" caption="Course director" />
              <Seal />
              <Signature name={date} caption="Date of issue" />
            </View>
          </View>
        </View>
        <Text className="absolute right-10 bottom-2 text-[6.5pt] text-stone-400">Certificate {id}</Text>
      </Page>
    </Document>
  );
}

export const sample: CertificateProps = {
  recipient: 'Ada Lovelace',
  course: 'Designing Documents with React',
  hours: 24,
  date: 'October 11, 2026',
  id: 'AA-2026-0815',
};

if (process.argv[1] === import.meta.filename)
  await renderToFile(<Certificate {...sample} />, 'certificate.pdf');
