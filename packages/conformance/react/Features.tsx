/**
 * A document that uses what the examples leave out: per-page content, `@media` blocks in a style,
 * Canvas, SVG text, an image by id, a note, form fields, a link, a page reference after a leader and a
 * bookmark title in the header. Each front end writes it again.
 */
import {
  BookmarkTitle,
  Canvas,
  Checkbox,
  Circle,
  Document,
  Footer,
  Header,
  Image,
  Leader,
  Link,
  Note,
  Page,
  PageRef,
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
        <Header>
          <Text className="text-gray-500">
            <BookmarkTitle />
          </Text>
        </Header>
        <Footer>
          <Text render={({ pageNumber, totalPages }) => `Page ${pageNumber} of ${totalPages}`} />
        </Footer>
        <Text bookmark={title} style={{ fontSize: 12, '@media (min-width: 400pt)': { fontSize: 18 } }}>
          {title}
        </Text>
        <View className="flex-row gap-2">
          {items.map((item, i) => (
            <Text key={item} className={i === 0 ? 'font-bold' : 'text-gray-500'}>
              {item}
            </Text>
          ))}
        </View>
        <Text>
          A{' '}
          <Link href="https://example.com" hitSlop={4}>
            link
          </Link>{' '}
          and a note <Note>Read me</Note>.
        </Text>
        <Text>
          The logo
          <Leader />
          <PageRef to="#logo" />
        </Text>
        <Image id="logo" src="logo" style={{ width: 24, height: 24 }} />
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
          <Checkbox
            name="agree"
            checked
            value="agreed"
            mark="cross"
            className="h-3 w-3 border border-gray-400"
          />
        </View>
      </Page>
    </Document>
  );
}
