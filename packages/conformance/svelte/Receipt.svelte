<script lang="ts">
  import { Document, Page, Text, View } from '@inkoxide/svelte';

  let { customer, lines }: { customer: string; lines: { name: string; amount: number }[] } = $props();
  const total = $derived(lines.reduce((sum, l) => sum + l.amount, 0));
</script>

<Document title={`Receipt for ${customer}`}>
  <Page size="A6" class="p-8 text-xs text-gray-900">
    <Text class="text-lg font-bold">Receipt</Text>
    <Text class="mb-4 text-gray-500">for {customer}</Text>
    {#each lines as l (l.name)}
      <View class="flex-row justify-between border-b border-gray-200 py-1.5">
        <Text>{l.name}</Text>
        <Text>${l.amount.toFixed(2)}</Text>
      </View>
    {/each}
    <View class="mt-3 flex-row justify-between font-bold">
      <Text>Total</Text>
      <Text>${total.toFixed(2)}</Text>
    </View>
  </Page>
</Document>
