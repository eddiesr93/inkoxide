<script setup lang="ts">
import { Checkbox, Document, Page, Radio, Select, Text, TextField, View } from '@inkoxide/vue';

import FormField from './FormField.vue';

defineProps<{ title: string; plans: string[] }>();

const input = 'h-6 w-full rounded border border-gray-300 bg-gray-50 px-1 text-[9pt]';
const consents = ['I agree to the terms of service.', 'Send me product news, at most once a month.'];
</script>

<template>
  <Document :title="title">
    <Page size="A4" class="p-16 text-[9pt] text-gray-900">
      <View class="mb-8 border-b-2 border-rose-500 pb-4">
        <Text class="text-2xl font-bold">{{ title }}</Text>
        <Text class="text-gray-500">Fill this form in your PDF viewer, then save and send it back.</Text>
      </View>

      <Text class="mb-3 text-sm font-bold">Your details</Text>
      <View class="mb-4 flex-row gap-4">
        <FormField label="First name"><TextField name="firstName" :class="input" /></FormField>
        <FormField label="Last name"><TextField name="lastName" :class="input" /></FormField>
      </View>
      <View class="mb-4 flex-row gap-4">
        <FormField label="Email"><TextField name="email" :class="input" /></FormField>
        <FormField label="Plan">
          <Select name="plan" :options="plans" :value="plans[0]" :class="input" />
        </FormField>
      </View>
      <View class="mb-8">
        <FormField label="Anything we should know?">
          <TextField
            name="notes"
            multiline
            class="h-20 w-full rounded border border-gray-300 bg-gray-50 p-1 text-[9pt]"
          />
        </FormField>
      </View>

      <Text class="mb-3 text-sm font-bold">Billing</Text>
      <View class="mb-8 flex-row gap-6">
        <View v-for="period in ['Monthly', 'Yearly']" :key="period" class="flex-row items-center gap-2">
          <Radio
            name="billing"
            :value="period"
            :checked="period === 'Yearly'"
            class="h-3.5 w-3.5 rounded-full border border-gray-400"
          />
          <Text>{{ period }}</Text>
        </View>
      </View>

      <View class="gap-2 rounded-lg bg-rose-50 p-4">
        <View v-for="(label, i) in consents" :key="label" class="flex-row items-center gap-2">
          <Checkbox :name="`consent${i}`" class="h-3.5 w-3.5 border border-gray-400 bg-white" />
          <Text>{{ label }}</Text>
        </View>
      </View>
    </Page>
  </Document>
</template>
