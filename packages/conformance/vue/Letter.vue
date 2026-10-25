<script setup lang="ts">
import { Document, Footer, Page, Text, View } from '@inkoxide/vue';

defineProps<{
  date: string;
  to: string[];
  greeting: string;
  paragraphs: string[];
  sender: { name: string; title: string };
}>();
</script>

<template>
  <Document title="Letter" :author="sender.name" lang="en-US">
    <Page size="Letter" class="px-24 pt-20 pb-24 text-[10.5pt] leading-relaxed text-slate-800">
      <Footer class="text-[7.5pt] text-slate-400">
        <View class="flex-row justify-between border-t border-slate-200 pt-3">
          <Text>Northwind Labs · 500 Market Street, San Francisco, CA 94105</Text>
          <Text href="mailto:hello@northwind.dev">hello@northwind.dev</Text>
        </View>
      </Footer>

      <View class="mb-14 flex-row items-center gap-2">
        <View class="h-3 w-3 rotate-45 bg-teal-600" />
        <Text class="text-base font-bold tracking-tight text-slate-900">Northwind Labs</Text>
      </View>

      <Text class="mb-8 text-slate-500">{{ date }}</Text>
      <View class="mb-8">
        <Text v-for="line in to" :key="line">{{ line }}</Text>
      </View>

      <Text class="mb-4">{{ greeting }}</Text>
      <Text v-for="p in paragraphs" :key="p.slice(0, 24)" class="mb-3 text-justify" hyphenate>
        {{ p }}
      </Text>

      <View class="mt-6" break="avoid">
        <Text>Kind regards,</Text>
        <Text class="mt-10 font-bold text-slate-900">{{ sender.name }}</Text>
        <Text class="text-slate-500">{{ sender.title }}</Text>
      </View>
    </Page>
  </Document>
</template>
