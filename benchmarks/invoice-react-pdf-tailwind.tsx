import { join } from 'node:path';

import { Document, Font, Image, Page, renderToFile, Text, View } from '@react-pdf/renderer';
import { createTw } from 'react-pdf-tailwind';

import invoice from './invoice.json' with { type: 'json' };

const FONTS = join(import.meta.dirname, '..', 'fixtures', 'fonts');
Font.register({
  family: 'Inter',
  fonts: [{ src: join(FONTS, 'Inter-Regular.ttf') }, { src: join(FONTS, 'Inter-Bold.ttf'), fontWeight: 700 }],
});
const tw = createTw({ fontFamily: { sans: ['Inter'] } });
const logo = join(import.meta.dirname, 'logo.jpg');
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
    <View style={tw('flex-1')}>
      <Text style={tw(label)}>{title}</Text>
      <Text style={tw('font-bold')}>{p.name}</Text>
      <Text style={tw('text-gray-500')}>
        Reg. no. {p.regNumber} · Tax ID {p.taxId}
      </Text>
      <Text style={tw('text-gray-500')}>{p.address}</Text>
      {p.account && (
        <Text style={tw('text-gray-500')}>
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
      {/* text-xs here hides the fixed footer (it sets a lineHeight); text-[9pt] does not. */}
      <Page size="A4" style={tw('p-16 font-sans text-[9pt] text-gray-900')}>
        <View
          style={tw('absolute right-16 bottom-6 left-16 flex-row justify-between text-[7.5pt] text-gray-500')}
          fixed
        >
          <Text>{inv.supplier.name}</Text>
          <Text render={({ pageNumber, totalPages }) => `Page ${pageNumber} of ${totalPages}`} />
        </View>

        <View style={tw('mb-9 flex-row items-start justify-between')}>
          <View style={tw('flex-row items-center gap-3')}>
            <Image src={logo} style={tw('h-11 w-11 object-contain')} />
            <Text style={tw('text-lg font-bold')}>{inv.supplier.name}</Text>
          </View>
          <View style={tw('items-end')}>
            <Text style={tw('text-2xl font-bold tracking-wider text-blue-900')}>INVOICE</Text>
            <Text style={tw('text-gray-500')}>
              No. {inv.series}-{inv.number}
            </Text>
          </View>
        </View>

        <View style={tw('mb-9 flex-row gap-6')}>
          <PartyBlock title="From" p={inv.supplier} />
          <PartyBlock title="Bill to" p={inv.client} />
          <View style={tw('w-36')}>
            <Text style={tw(label)}>Issue date</Text>
            <Text>{inv.issueDate}</Text>
            <Text style={tw(`mt-2 ${label}`)}>Due date</Text>
            <Text>{inv.dueDate}</Text>
          </View>
        </View>

        <View style={tw('flex-row')} fixed>
          <Text style={tw(`${th} flex-1`)}>Description</Text>
          <Text style={tw(`${th} w-[44pt] text-right`)}>Qty</Text>
          <Text style={tw(`${th} w-[66pt] text-right`)}>Unit price</Text>
          <Text style={tw(`${th} w-[72pt] text-right`)}>Amount</Text>
        </View>
        {lines.map((l) => (
          <View key={l.name} style={tw('flex-row')} wrap={false}>
            <View style={tw(`${td} flex-1`)}>
              <Text style={tw('font-bold')}>{l.name}</Text>
              <Text style={tw('text-gray-500')}>{l.details}</Text>
            </View>
            <Text style={tw(`${td} w-[44pt] text-right`)}>{l.quantity}</Text>
            <Text style={tw(`${td} w-[66pt] text-right`)}>{money.format(l.unitPrice)}</Text>
            <Text style={tw(`${td} w-[72pt] text-right font-bold`)}>{money.format(l.net)}</Text>
          </View>
        ))}

        <View style={tw('mt-3 items-end gap-1')}>
          <Text style={tw('text-gray-500')}>
            Subtotal {money.format(net)} {inv.currency}
          </Text>
          <Text style={tw('text-gray-500')}>
            VAT {inv.vatRate}% {money.format(vat)} {inv.currency}
          </Text>
          <Text style={tw('text-sm font-bold text-blue-900')}>
            Total due {money.format(net + vat)} {inv.currency}
          </Text>
        </View>

        <View style={tw('mt-6 gap-1 rounded bg-gray-50 p-3')} wrap={false}>
          <Text style={tw(label)}>Notes</Text>
          {inv.notes.map((n) => (
            <Text key={n}>{n}</Text>
          ))}
        </View>
      </Page>
    </Document>
  );
}

// Run directly: writes the PDF. Imported (bench.tsx): only exports Invoice.
if (process.argv[1] === import.meta.filename)
  await renderToFile(<Invoice {...invoice} />, 'invoice-react-pdf-tailwind.pdf');
