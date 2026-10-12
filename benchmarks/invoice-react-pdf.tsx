import { realpathSync } from 'node:fs';
import { join } from 'node:path';

import { Document, Font, Image, Page, renderToFile, StyleSheet, Text, View } from '@react-pdf/renderer';

import invoice from './invoice.json' with { type: 'json' };
import { setUpReactPdf } from './report.tsx';

const logo = join(import.meta.dirname, 'logo.jpg');
const money = new Intl.NumberFormat('en-US', { minimumFractionDigits: 2 });

const styles = StyleSheet.create({
  page: { padding: 48, fontFamily: 'Inter', fontSize: 9, color: '#111827' },
  footer: {
    position: 'absolute',
    bottom: 20,
    left: 48,
    right: 48,
    flexDirection: 'row',
    justifyContent: 'space-between',
    fontSize: 7.5,
    color: '#6b7280',
  },
  header: {
    marginBottom: 27,
    flexDirection: 'row',
    alignItems: 'flex-start',
    justifyContent: 'space-between',
  },
  brand: { flexDirection: 'row', alignItems: 'center', gap: 9 },
  logo: { width: 33, height: 33, objectFit: 'contain' },
  company: { fontSize: 13.5, fontWeight: 700 },
  title: { fontSize: 18, fontWeight: 700, letterSpacing: 0.9, color: '#1e3a8a' },
  right: { alignItems: 'flex-end' },
  muted: { color: '#6b7280' },
  bold: { fontWeight: 700 },
  parties: { marginBottom: 27, flexDirection: 'row', gap: 18 },
  party: { flex: 1 },
  dates: { width: 108 },
  label: {
    marginBottom: 3,
    fontSize: 7.5,
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: 0.19,
    color: '#6b7280',
  },
  spaced: { marginTop: 6 },
  row: { flexDirection: 'row' },
  th: {
    borderBottomWidth: 0.75,
    borderBottomColor: '#d1d5db',
    backgroundColor: '#f9fafb',
    padding: 6,
    fontSize: 8,
    fontWeight: 700,
    color: '#6b7280',
  },
  td: { borderBottomWidth: 0.75, borderBottomColor: '#e5e7eb', padding: 6 },
  desc: { flex: 1 },
  qty: { width: 44, textAlign: 'right' },
  price: { width: 66, textAlign: 'right' },
  amount: { width: 72, textAlign: 'right' },
  totals: { marginTop: 9, alignItems: 'flex-end', gap: 3 },
  total: { fontSize: 10.5, fontWeight: 700, color: '#1e3a8a' },
  notes: { marginTop: 18, gap: 3, borderRadius: 3, backgroundColor: '#f9fafb', padding: 9 },
});

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
    <View style={styles.party}>
      <Text style={styles.label}>{title}</Text>
      <Text style={styles.bold}>{p.name}</Text>
      <Text style={styles.muted}>
        Reg. no. {p.regNumber} · Tax ID {p.taxId}
      </Text>
      <Text style={styles.muted}>{p.address}</Text>
      {p.account && (
        <Text style={styles.muted}>
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
      <Page size="A4" style={styles.page}>
        <View style={styles.footer} fixed>
          <Text>{inv.supplier.name}</Text>
          <Text render={({ pageNumber, totalPages }) => `Page ${pageNumber} of ${totalPages}`} />
        </View>

        <View style={styles.header}>
          <View style={styles.brand}>
            <Image src={logo} style={styles.logo} />
            <Text style={styles.company}>{inv.supplier.name}</Text>
          </View>
          <View style={styles.right}>
            <Text style={styles.title}>INVOICE</Text>
            <Text style={styles.muted}>
              No. {inv.series}-{inv.number}
            </Text>
          </View>
        </View>

        <View style={styles.parties}>
          <PartyBlock title="From" p={inv.supplier} />
          <PartyBlock title="Bill to" p={inv.client} />
          <View style={styles.dates}>
            <Text style={styles.label}>Issue date</Text>
            <Text>{inv.issueDate}</Text>
            <Text style={[styles.label, styles.spaced]}>Due date</Text>
            <Text>{inv.dueDate}</Text>
          </View>
        </View>

        <View style={styles.row} fixed>
          <Text style={[styles.th, styles.desc]}>Description</Text>
          <Text style={[styles.th, styles.qty]}>Qty</Text>
          <Text style={[styles.th, styles.price]}>Unit price</Text>
          <Text style={[styles.th, styles.amount]}>Amount</Text>
        </View>
        {lines.map((l) => (
          <View key={l.name} style={styles.row} wrap={false}>
            <View style={[styles.td, styles.desc]}>
              <Text style={styles.bold}>{l.name}</Text>
              <Text style={styles.muted}>{l.details}</Text>
            </View>
            <Text style={[styles.td, styles.qty]}>{l.quantity}</Text>
            <Text style={[styles.td, styles.price]}>{money.format(l.unitPrice)}</Text>
            <Text style={[styles.td, styles.amount, styles.bold]}>{money.format(l.net)}</Text>
          </View>
        ))}

        <View style={styles.totals}>
          <Text style={styles.muted}>
            Subtotal {money.format(net)} {inv.currency}
          </Text>
          <Text style={styles.muted}>
            VAT {inv.vatRate}% {money.format(vat)} {inv.currency}
          </Text>
          <Text style={styles.total}>
            Total due {money.format(net + vat)} {inv.currency}
          </Text>
        </View>

        <View style={styles.notes} wrap={false}>
          <Text style={styles.label}>Notes</Text>
          {inv.notes.map((n) => (
            <Text key={n}>{n}</Text>
          ))}
        </View>
      </Page>
    </Document>
  );
}

// Run directly, by its path or through a symlink: writes the PDF, with the font setup bench.tsx
// uses. Imported (bench.tsx): only exports Invoice.
if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  setUpReactPdf(Font);
  await renderToFile(<Invoice {...invoice} />, 'invoice-react-pdf.pdf');
}
