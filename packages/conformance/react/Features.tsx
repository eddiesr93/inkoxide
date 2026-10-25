/**
 * A document that uses what the examples leave out: per-page content, `@media` blocks in a style,
 * Canvas, SVG text, an image by id, a note, form fields and a link. Each front end writes it again.
 */
import {
  Canvas,
  Checkbox,
  Circle,
  Document,
  Footer,
  Image,
  Link,
  Note,
  Page,
  Svg,
  Text,
  TextField,
  Tspan,
  View,
} from '@inkoxide/react';

export const sample = { title: 'Features', items: ['One', 'Two', 'Three'] };

export default function Features({ title, items }: typeof sample) {
  return (
    <Document title={title}>
      <Page size="A5" style={{ padding: 30 }}>
        <Footer>
          <Text render={({ pageNumber, totalPages }) => `Page ${pageNumber} of ${totalPages}`} />
        </Footer>
        <Text style={{ fontSize: 12, '@media (min-width: 400pt)': { fontSize: 18 } }}>{title}</Text>
        <View className="flex-row gap-2">
          {items.map((item, i) => (
            <Text key={item} className={i === 0 ? 'font-bold' : 'text-gray-500'}>
              {item}
            </Text>
          ))}
        </View>
        <Text>
          A <Link href="https://example.com">link</Link> and a note <Note>Read me</Note>.
        </Text>
        <Image src="logo" style={{ width: 24, height: 24 }} />
        <Svg width={60} height={30} viewBox="0 0 60 30">
          <Circle cx={15} cy={15} r={10} fill="#0ea5e9" />
          <Text x={30} y={20} fontSize={10}>
            S<Tspan fill="red">vg</Tspan>
          </Text>
        </Svg>
        <Canvas
          width={40}
          height={20}
          paint={(p, w, h) => {
            p.rect(0, 0, w, h).fill('#fde68a');
          }}
        />
        <View className="flex-row gap-2">
          <TextField name="name" value="Ada" className="h-5 w-24 border border-gray-300" />
          <Checkbox name="agree" checked className="h-3 w-3 border border-gray-400" />
        </View>
      </Page>
    </Document>
  );
}
