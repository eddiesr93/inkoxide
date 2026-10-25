<script setup lang="ts">
import { Document, Page, Text, View } from '@inkoxide/vue';

import Section from './Section.vue';

defineProps<{
  name: string;
  title: string;
  contact: string[];
  summary: string;
  skills: { group: string; items: string[] }[];
  jobs: { role: string; company: string; period: string; points: string[] }[];
  education: { degree: string; school: string; year: string }[];
}>();

const initials = (name: string) =>
  name
    .split(' ')
    .map((n) => n[0])
    .join('');
</script>

<template>
  <Document :title="`${name} — Résumé`" :author="name">
    <Page size="A4" class="text-[8.5pt] leading-snug text-slate-800">
      <View class="grid flex-1 grid-cols-[170pt_1fr]">
        <View class="bg-slate-900 px-8 py-12 text-slate-300">
          <View class="mb-5 h-16 w-16 items-center justify-center rounded-full bg-sky-500">
            <Text class="text-xl font-bold text-white">{{ initials(name) }}</Text>
          </View>
          <Text class="text-xl font-bold leading-tight text-white">{{ name }}</Text>
          <Text class="mb-8 text-sky-300">{{ title }}</Text>
          <Text class="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">Contact</Text>
          <View class="mb-8 gap-1">
            <Text v-for="c in contact" :key="c">{{ c }}</Text>
          </View>
          <View v-for="s in skills" :key="s.group" class="mb-5">
            <Text class="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">{{
              s.group
            }}</Text>
            <View class="flex-row flex-wrap gap-1">
              <Text
                v-for="it in s.items"
                :key="it"
                class="rounded bg-slate-800 px-1.5 py-0.5 text-[7.5pt] text-slate-200"
                >{{ it }}</Text
              >
            </View>
          </View>
        </View>
        <View class="px-10 py-12">
          <Section title="Profile">
            <Text class="text-[9pt] leading-relaxed text-slate-600">{{ summary }}</Text>
          </Section>
          <Section title="Experience">
            <View v-for="j in jobs" :key="j.company" class="mb-4" break="avoid">
              <View class="flex-row items-baseline justify-between">
                <Text class="text-[10pt] font-bold text-slate-900">{{ j.role }}</Text>
                <Text class="text-slate-400">{{ j.period }}</Text>
              </View>
              <Text class="mb-1.5 text-sky-700">{{ j.company }}</Text>
              <View v-for="p in j.points" :key="p" class="mb-0.5 flex-row gap-2">
                <Text class="text-sky-500">•</Text>
                <Text class="flex-1 text-slate-600">{{ p }}</Text>
              </View>
            </View>
          </Section>
          <Section title="Education">
            <View v-for="e in education" :key="e.degree" class="flex-row justify-between">
              <View>
                <Text class="font-bold text-slate-900">{{ e.degree }}</Text>
                <Text class="text-slate-500">{{ e.school }}</Text>
              </View>
              <Text class="text-slate-400">{{ e.year }}</Text>
            </View>
          </Section>
        </View>
      </View>
    </Page>
  </Document>
</template>
