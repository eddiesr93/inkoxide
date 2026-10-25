<script setup lang="ts">
import { Text, View } from '@inkoxide/vue';

defineProps<{ title: string }>();
</script>

<template>
  <View class="mb-6">
    <Text class="mb-3 text-[7.5pt] font-bold uppercase tracking-widest text-sky-700">{{ title }}</Text>
    <slot />
  </View>
</template>
