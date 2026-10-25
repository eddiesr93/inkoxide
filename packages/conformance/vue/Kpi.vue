<script setup lang="ts">
import { Text, View } from '@inkoxide/vue';

defineProps<{ label: string; value: string; note: string }>();
</script>

<template>
  <View class="flex-1 rounded-lg border border-gray-200 p-4">
    <Text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-500">{{ label }}</Text>
    <Text class="mt-1 text-xl font-bold">{{ value }}</Text>
    <Text class="text-emerald-600">{{ note }}</Text>
  </View>
</template>
