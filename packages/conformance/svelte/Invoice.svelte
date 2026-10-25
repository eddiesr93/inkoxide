<script lang="ts">
  import { Cell, Circle, Document, Footer, Page, PageNumber, Path, Row, Svg, Table, Text, TotalPages, View } from '@inkoxide/svelte';

  import Address from './Address.svelte';

  interface Party {
    name: string;
    address: string[];
    email: string;
  }

  let {
    number,
    issued,
    due,
    from,
    to,
    items,
    taxRate,
  }: {
    number: string;
    issued: string;
    due: string;
    from: Party;
    to: Party;
    items: { description: string; details?: string; quantity: number; price: number }[];
    taxRate: number;
  } = $props();

  const money = (n: number) => n.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
  const percent = (n: number) => n.toLocaleString('en-US', { style: 'percent', maximumFractionDigits: 2 });
  const subtotal = $derived(items.reduce((sum, it) => sum + it.quantity * it.price, 0));
  const tax = $derived(subtotal * taxRate);
  const th = 'border-b border-gray-300 py-2 text-[7.5pt] font-bold uppercase tracking-wider text-gray-500';
  const td = 'border-b border-gray-100 py-2.5';
</script>

<Document title={`Invoice ${number}`} author={from.name}>
  <Page size="A4" class="px-16 pt-14 pb-16 text-[9pt] text-gray-900">
    <Footer class="flex-row justify-between text-[7.5pt] text-gray-400">
      <Text>{from.name} · {from.email}</Text>
      <Text class="text-right">Page <PageNumber /> of <TotalPages /></Text>
    </Footer>

    <View class="mb-12 flex-row items-start justify-between">
      <View class="flex-row items-center gap-2.5">
        <Svg width={28} height={28} viewBox="0 0 32 32">
          <Circle cx={16} cy={16} r={16} fill="#4f46e5" />
          <Path d="M10 21 L16 9 L22 21 M12.5 16.5 H19.5" fill="none" stroke="white" strokeWidth={2.4} strokeLinecap="round" />
        </Svg>
        <Text class="text-base font-bold">{from.name}</Text>
      </View>
      <View class="items-end">
        <Text class="text-3xl font-bold tracking-tight text-indigo-600">Invoice</Text>
        <Text class="text-gray-500">#{number}</Text>
      </View>
    </View>

    <View class="mb-10 flex-row gap-8">
      <Address label="From" party={from} />
      <Address label="Bill to" party={to} />
      <View class="w-32 gap-3">
        <View>
          <Text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">Issued</Text>
          <Text>{issued}</Text>
        </View>
        <View>
          <Text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">Due</Text>
          <Text class="font-bold">{due}</Text>
        </View>
      </View>
    </View>

    <Table columns={['1fr', 50, 80, 90]} headerRows={1}>
      <Row>
        <Cell class={th}>Description</Cell>
        <Cell class={`${th} text-right`}>Qty</Cell>
        <Cell class={`${th} text-right`}>Price</Cell>
        <Cell class={`${th} text-right`}>Amount</Cell>
      </Row>
      {#each items as it (it.description)}
        <Row>
          <Cell class={td}>
            <Text class="font-bold">{it.description}</Text>
            {#if it.details}<Text class="text-gray-500">{it.details}</Text>{/if}
          </Cell>
          <Cell class={`${td} text-right`}>{it.quantity}</Cell>
          <Cell class={`${td} text-right`}>{money(it.price)}</Cell>
          <Cell class={`${td} text-right font-bold`}>{money(it.quantity * it.price)}</Cell>
        </Row>
      {/each}
    </Table>

    <View class="mt-6 ml-auto w-56 gap-1.5" break="avoid">
      <View class="flex-row justify-between text-gray-600">
        <Text>Subtotal</Text>
        <Text>{money(subtotal)}</Text>
      </View>
      <View class="flex-row justify-between text-gray-600">
        <Text>Tax ({percent(taxRate)})</Text>
        <Text>{money(tax)}</Text>
      </View>
      <View class="mt-1 flex-row justify-between rounded-md bg-indigo-50 px-3 py-2.5 text-sm font-bold text-indigo-700">
        <Text>Total due</Text>
        <Text>{money(subtotal + tax)}</Text>
      </View>
    </View>

    <View class="mt-12 rounded-md bg-gray-50 p-4 text-gray-600" break="avoid">
      <Text class="font-bold text-gray-900">Payment</Text>
      <Text>Bank transfer within 30 days. Please quote #{number} with your payment.</Text>
    </View>
  </Page>
</Document>
