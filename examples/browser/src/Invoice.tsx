// The invoice the page renders. It imports from `@inkoxide/render`: in the page, vite.config.ts
// points that at `@inkoxide/render/browser`; the tests render it in Node with the main entry, and
// compare the two PDFs.
import {
  Cell,
  Circle,
  Document,
  Footer,
  Page,
  PageNumber,
  Path,
  Row,
  Svg,
  Table,
  Text,
  TotalPages,
  View,
} from '@inkoxide/render';

export interface Item {
  description: string;
  quantity: number;
  price: number;
}

export interface InvoiceProps {
  number: string;
  customer: string;
  items: Item[];
}

const money = (n: number) => `$${n.toFixed(2)}`;

export default function Invoice({ number, customer, items }: InvoiceProps) {
  const total = items.reduce((sum, it) => sum + it.quantity * it.price, 0);
  return (
    <Document title={`Invoice ${number}`}>
      <Page size="A4" className="px-16 pt-14 pb-16 text-[9pt] text-gray-900">
        <Footer className="flex-row justify-between text-[7.5pt] text-gray-400">
          <Text>Acme Studio</Text>
          <Text>
            Page <PageNumber /> of <TotalPages />
          </Text>
        </Footer>
        <View className="mb-10 flex-row items-center justify-between">
          <Svg width={28} height={28} viewBox="0 0 32 32">
            <Circle cx={16} cy={16} r={16} fill="#4f46e5" />
            <Path
              d="M10 21 L16 9 L22 21"
              fill="none"
              stroke="white"
              strokeWidth={2.4}
              strokeLinecap="round"
            />
          </Svg>
          <Text className="text-3xl font-bold text-indigo-600">Invoice #{number}</Text>
        </View>
        <Text className="mb-6">Bill to {customer}</Text>
        <Table columns={['1fr', 50, 90]} headerRows={1}>
          <Row>
            <Cell className="border-b border-gray-300 py-2 font-bold">Description</Cell>
            <Cell className="border-b border-gray-300 py-2 text-right font-bold">Qty</Cell>
            <Cell className="border-b border-gray-300 py-2 text-right font-bold">Amount</Cell>
          </Row>
          {items.map((it) => (
            <Row key={it.description}>
              <Cell className="border-b border-gray-100 py-2.5">{it.description}</Cell>
              <Cell className="border-b border-gray-100 py-2.5 text-right">{it.quantity}</Cell>
              <Cell className="border-b border-gray-100 py-2.5 text-right">
                {money(it.quantity * it.price)}
              </Cell>
            </Row>
          ))}
        </Table>
        <Text className="mt-6 text-right text-sm font-bold">Total {money(total)}</Text>
        <Text className="mt-12 italic text-gray-600" style={{ fontFamily: 'Roboto Mono' }}>
          Thank you.
        </Text>
      </Page>
    </Document>
  );
}

export const sample: InvoiceProps = {
  number: '2026-0142',
  customer: 'Lincoln Elementary',
  items: Array.from({ length: 40 }, (_, k) => ({
    description: `Line item ${k + 1}`,
    quantity: (k % 5) + 1,
    price: 25 + k * 3.5,
  })),
};

/** The options both renders use: a fixed date, so that their bytes can be compared. */
export const OPTIONS = { creationDate: '2026-10-08T00:00:00Z' };
