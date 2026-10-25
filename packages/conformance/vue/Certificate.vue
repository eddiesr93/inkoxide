<script setup lang="ts">
import { Circle, Document, Page, Path, Svg, Text, View } from '@inkoxide/vue';

import Signature from './Signature.vue';

defineProps<{ recipient: string; course: string; hours: number; date: string; id: string }>();

const points = Array.from({ length: 24 }, (_, i) => {
  const a = (i * Math.PI) / 12;
  const r = i % 2 === 0 ? 46 : 40;
  return `${50 + r * Math.cos(a)},${50 + r * Math.sin(a)}`;
});
</script>

<template>
  <Document :title="`Certificate: ${recipient}`">
    <Page size="A4-landscape" class="bg-amber-50 p-6 text-[9pt] text-stone-800">
      <View class="flex-1 border-2 border-amber-700 p-1.5">
        <View class="flex-1 items-center border border-amber-700/50 px-20 py-12">
          <Text class="text-[8pt] font-bold uppercase tracking-[0.4em] text-amber-700">Acme Academy</Text>
          <View class="flex-1 items-center justify-center">
            <Text class="font-display text-[40pt] text-stone-900">Certificate of Completion</Text>
            <Text class="mt-4 text-sm text-stone-500">This certifies that</Text>
            <Text
              class="mt-4 border-b border-amber-700/40 px-10 pb-2 font-display text-[30pt] text-amber-800"
            >
              {{ recipient }}
            </Text>
            <Text class="mt-5 max-w-md text-center text-sm leading-relaxed text-stone-600">
              has completed <Text class="font-bold text-stone-900">{{ course }}</Text
              >, a {{ hours }}-hour course with practical assessments, and is awarded this certificate.
            </Text>
          </View>
          <View class="w-full flex-row items-end justify-between">
            <Signature name="Grace Hopper" caption="Course director" />
            <Svg :width="84" :height="84" view-box="0 0 100 100">
              <Path :d="`M${points.join(' L')} Z`" fill="#b45309" />
              <Circle :cx="50" :cy="50" :r="34" fill="#f59e0b" />
              <Circle :cx="50" :cy="50" :r="28" fill="none" stroke="#fef3c7" :stroke-width="1.5" />
              <Path
                d="M38 51 L46 59 L63 41"
                fill="none"
                stroke="white"
                :stroke-width="5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </Svg>
            <Signature :name="date" caption="Date of issue" />
          </View>
        </View>
      </View>
      <Text class="absolute right-10 bottom-2 text-[6.5pt] text-stone-400">Certificate {{ id }}</Text>
    </Page>
  </Document>
</template>
