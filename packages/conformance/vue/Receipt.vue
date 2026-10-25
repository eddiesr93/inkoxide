<script setup lang="ts">
import { Document, Page, Text, View } from '@inkoxide/vue';

interface Line {
  name: string;
  amount: number;
}

const props = defineProps<{ customer: string; lines: Line[] }>();
const total = props.lines.reduce((sum, l) => sum + l.amount, 0);
</script>

<template>
  <Document :title="`Receipt for ${customer}`">
    <Page size="A6" class="p-8 text-xs text-gray-900">
      <Text class="text-lg font-bold">Receipt</Text>
      <Text class="mb-4 text-gray-500">for {{ customer }}</Text>
      <View v-for="l in lines" :key="l.name" class="flex-row justify-between border-b border-gray-200 py-1.5">
        <Text>{{ l.name }}</Text>
        <Text>${{ l.amount.toFixed(2) }}</Text>
      </View>
      <View class="mt-3 flex-row justify-between font-bold">
        <Text>Total</Text>
        <Text>${{ total.toFixed(2) }}</Text>
      </View>
    </Page>
  </Document>
</template>
