import { realpathSync } from 'node:fs';

import {
  Cell,
  Circle,
  Document,
  Footer,
  Page,
  PageNumber,
  Path,
  Row,
  renderToFile,
  Svg,
  Table,
  Text,
  TotalPages,
  View,
} from '@inkoxide/render';

interface Party {
  name: string;
  address: string[];
  email: string;
}

interface Item {
  description: string;
  details?: string;
  quantity: number;
  price: number;
}

export interface InvoiceProps {
  number: string;
  issued: string;
  due: string;
  from: Party;
  to: Party;
  items: Item[];
  taxRate: number;
}

const money = (n: number) => n.toLocaleString('en-US', { style: 'currency', currency: 'USD' });

function Logo() {
  return (
    <Svg width={28} height={28} viewBox="0 0 32 32">
      <Circle cx={16} cy={16} r={16} fill="#4f46e5" />
      <Path
        d="M10 21 L16 9 L22 21 M12.5 16.5 H19.5"
        fill="none"
        stroke="white"
        strokeWidth={2.4}
        strokeLinecap="round"
      />
    </Svg>
  );
}

function Address({ label, party }: { label: string; party: Party }) {
  return (
    <View className="flex-1">
      <Text className="mb-1 text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">{label}</Text>
      <Text className="font-bold">{party.name}</Text>
      {party.address.map((line) => (
        <Text key={line} className="text-gray-600">
          {line}
        </Text>
      ))}
      <Text className="text-gray-600">{party.email}</Text>
    </View>
  );
}

export default function Invoice({ number, issued, due, from, to, items, taxRate }: InvoiceProps) {
  const subtotal = items.reduce((sum, it) => sum + it.quantity * it.price, 0);
  const tax = subtotal * taxRate;
  const th = 'border-b border-gray-300 py-2 text-[7.5pt] font-bold uppercase tracking-wider text-gray-500';
  const td = 'border-b border-gray-100 py-2.5';

  return (
    <Document title={`Invoice ${number}`} author={from.name}>
      <Page size="A4" className="px-16 pt-14 pb-16 text-[9pt] text-gray-900">
        <Footer className="flex-row justify-between text-[7.5pt] text-gray-400">
          <Text>
            {from.name} · {from.email}
          </Text>
          <Text className="text-right">
            Page <PageNumber /> of <TotalPages />
          </Text>
        </Footer>

        <View className="mb-12 flex-row items-start justify-between">
          <View className="flex-row items-center gap-2.5">
            <Logo />
            <Text className="text-base font-bold">{from.name}</Text>
          </View>
          <View className="items-end">
            <Text className="text-3xl font-bold tracking-tight text-indigo-600">Invoice</Text>
            <Text className="text-gray-500">#{number}</Text>
          </View>
        </View>

        <View className="mb-10 flex-row gap-8">
          <Address label="From" party={from} />
          <Address label="Bill to" party={to} />
          <View className="w-32 gap-3">
            <View>
              <Text className="text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">Issued</Text>
              <Text>{issued}</Text>
            </View>
            <View>
              <Text className="text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">Due</Text>
              <Text className="font-bold">{due}</Text>
            </View>
          </View>
        </View>

        <Table columns={['1fr', 50, 80, 90]} headerRows={1}>
          <Row>
            <Cell className={th}>Description</Cell>
            <Cell className={`${th} text-right`}>Qty</Cell>
            <Cell className={`${th} text-right`}>Price</Cell>
            <Cell className={`${th} text-right`}>Amount</Cell>
          </Row>
          {items.map((it) => (
            <Row key={it.description}>
              <Cell className={td}>
                <Text className="font-bold">{it.description}</Text>
                {it.details && <Text className="text-gray-500">{it.details}</Text>}
              </Cell>
              <Cell className={`${td} text-right`}>{it.quantity}</Cell>
              <Cell className={`${td} text-right`}>{money(it.price)}</Cell>
              <Cell className={`${td} text-right font-bold`}>{money(it.quantity * it.price)}</Cell>
            </Row>
          ))}
        </Table>

        <View className="mt-6 ml-auto w-56 gap-1.5" break="avoid">
          <View className="flex-row justify-between text-gray-600">
            <Text>Subtotal</Text>
            <Text>{money(subtotal)}</Text>
          </View>
          <View className="flex-row justify-between text-gray-600">
            <Text>Tax ({taxRate * 100}%)</Text>
            <Text>{money(tax)}</Text>
          </View>
          <View className="mt-1 flex-row justify-between rounded-md bg-indigo-50 px-3 py-2.5 text-sm font-bold text-indigo-700">
            <Text>Total due</Text>
            <Text>{money(subtotal + tax)}</Text>
          </View>
        </View>

        <View className="mt-12 rounded-md bg-gray-50 p-4 text-gray-600" break="avoid">
          <Text className="font-bold text-gray-900">Payment</Text>
          <Text>Bank transfer within 30 days. Please quote #{number} with your payment.</Text>
        </View>
      </Page>
    </Document>
  );
}

export const sample: InvoiceProps = {
  number: '2026-0142',
  issued: 'October 11, 2026',
  due: 'November 10, 2026',
  from: {
    name: 'Acme Studio',
    address: ['25 Harbor Street', 'Portland, OR 97204'],
    email: 'billing@acme.studio',
  },
  to: {
    name: 'Lincoln Elementary',
    address: ['4 Maple Avenue', 'Springfield, IL 62704'],
    email: 'office@lincoln.edu',
  },
  items: [
    { description: 'Brand identity', details: 'Logo, palette and type system', quantity: 1, price: 4800 },
    { description: 'Website design', details: 'Eight pages, desktop and mobile', quantity: 8, price: 650 },
    { description: 'Illustrations', details: 'Custom spot illustrations', quantity: 12, price: 180 },
    { description: 'Support', details: 'Hours, billed monthly', quantity: 6, price: 95 },
  ],
  taxRate: 0.08,
};

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename)
  await renderToFile(<Invoice {...sample} />, 'invoice.pdf');
