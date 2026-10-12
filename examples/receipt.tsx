import { realpathSync } from 'node:fs';

import { Document, Page, renderToFile, Text, View } from '@inkoxide/render';

interface Line {
  name: string;
  amount: number;
}

export default function Receipt({ customer, lines }: { customer: string; lines: Line[] }) {
  const total = lines.reduce((sum, l) => sum + l.amount, 0);
  return (
    <Document title={`Receipt for ${customer}`}>
      <Page size="A6" className="p-8 text-xs text-gray-900">
        <Text className="text-lg font-bold">Receipt</Text>
        <Text className="mb-4 text-gray-500">for {customer}</Text>
        {lines.map((l) => (
          <View key={l.name} className="flex-row justify-between border-b border-gray-200 py-1.5">
            <Text>{l.name}</Text>
            <Text>${l.amount.toFixed(2)}</Text>
          </View>
        ))}
        <View className="mt-3 flex-row justify-between font-bold">
          <Text>Total</Text>
          <Text>${total.toFixed(2)}</Text>
        </View>
      </Page>
    </Document>
  );
}

export const sample = {
  customer: 'Ada Lovelace',
  lines: [
    { name: 'Espresso', amount: 3.2 },
    { name: 'Almond croissant', amount: 4.5 },
    { name: 'Sparkling water', amount: 2 },
  ],
};

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename)
  await renderToFile(<Receipt {...sample} />, 'receipt.pdf');
