import { readFileSync, realpathSync } from 'node:fs';

import {
  Cell,
  Document,
  Footer,
  Image,
  Page,
  PageNumber,
  Row,
  renderToFile,
  Table,
  Text,
  TotalPages,
  View,
} from '@inkoxide/render';

import invoice from './invoice.json' with { type: 'json' };

const logo = readFileSync(new URL('./logo.jpg', import.meta.url));
const money = new Intl.NumberFormat('en-US', { minimumFractionDigits: 2 });

const label = 'mb-1 text-[7.5pt] font-bold uppercase tracking-wide text-gray-500';
const th = 'border-b border-gray-300 bg-gray-50 p-2 text-[8pt] font-bold text-gray-500';
const td = 'border-b border-gray-200 p-2';

interface Party {
  name: string;
  regNumber: string;
  taxId: string;
  address: string;
  account?: string;
  bank?: string;
}

function PartyBlock({ title, p }: { title: string; p: Party }) {
  return (
    <View className="flex-1">
      <Text className={label}>{title}</Text>
      <Text className="font-bold">{p.name}</Text>
      <Text className="text-gray-500">
        Reg. no. {p.regNumber} · Tax ID {p.taxId}
      </Text>
      <Text className="text-gray-500">{p.address}</Text>
      {p.account && (
        <Text className="text-gray-500">
          Account {p.account} · {p.bank}
        </Text>
      )}
    </View>
  );
}

export function Invoice(inv: typeof invoice) {
  const lines = inv.items.map((it) => ({ ...it, net: it.quantity * it.unitPrice }));
  const net = lines.reduce((a, l) => a + l.net, 0);
  const vat = (net * inv.vatRate) / 100;
  return (
    <Document title={`Invoice ${inv.series}-${inv.number}`}>
      <Page size="A4" className="p-16 text-xs text-gray-900">
        <Footer className="flex-row justify-between text-[7.5pt] text-gray-500">
          <Text>{inv.supplier.name}</Text>
          <Text>
            Page <PageNumber /> of <TotalPages />
          </Text>
        </Footer>

        <View className="mb-9 flex-row items-start justify-between">
          <View className="flex-row items-center gap-3">
            <Image src={logo} className="h-11 w-11" fit="contain" />
            <Text className="text-lg font-bold">{inv.supplier.name}</Text>
          </View>
          <View className="items-end">
            <Text className="text-2xl font-bold tracking-wider text-blue-900">INVOICE</Text>
            <Text className="text-gray-500">
              No. {inv.series}-{inv.number}
            </Text>
          </View>
        </View>

        <View className="mb-9 flex-row gap-6">
          <PartyBlock title="From" p={inv.supplier} />
          <PartyBlock title="Bill to" p={inv.client} />
          <View className="w-36">
            <Text className={label}>Issue date</Text>
            <Text>{inv.issueDate}</Text>
            <Text className={`mt-2 ${label}`}>Due date</Text>
            <Text>{inv.dueDate}</Text>
          </View>
        </View>

        <Table columns={['1fr', 44, 66, 72]} headerRows={1}>
          <Row>
            <Cell className={th}>Description</Cell>
            <Cell className={`${th} text-right`}>Qty</Cell>
            <Cell className={`${th} text-right`}>Unit price</Cell>
            <Cell className={`${th} text-right`}>Amount</Cell>
          </Row>
          {lines.map((l) => (
            <Row key={l.name}>
              <Cell className={td}>
                <Text className="font-bold">{l.name}</Text>
                <Text className="text-gray-500">{l.details}</Text>
              </Cell>
              <Cell className={`${td} text-right`}>{l.quantity}</Cell>
              <Cell className={`${td} text-right`}>{money.format(l.unitPrice)}</Cell>
              <Cell className={`${td} text-right font-bold`}>{money.format(l.net)}</Cell>
            </Row>
          ))}
        </Table>

        <View className="mt-3 items-end gap-1">
          <Text className="text-gray-500">
            Subtotal {money.format(net)} {inv.currency}
          </Text>
          <Text className="text-gray-500">
            VAT {inv.vatRate}% {money.format(vat)} {inv.currency}
          </Text>
          <Text className="text-sm font-bold text-blue-900">
            Total due {money.format(net + vat)} {inv.currency}
          </Text>
        </View>

        <View className="mt-6 gap-1 rounded bg-gray-50 p-3" break="avoid">
          <Text className={label}>Notes</Text>
          {inv.notes.map((n) => (
            <Text key={n}>{n}</Text>
          ))}
        </View>
      </Page>
    </Document>
  );
}

// Run directly, by its path or through a symlink: writes the PDF. Imported (bench.tsx): only
// exports Invoice.
if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename)
  await renderToFile(<Invoice {...invoice} />, 'invoice-inkoxide.pdf');
