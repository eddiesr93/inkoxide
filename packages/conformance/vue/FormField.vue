<script setup lang="ts">
import { Text, View } from '@inkoxide/vue';

defineProps<{ label: string }>();
</script>

<template>
  <View class="flex-1 gap-1">
    <Text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-500">{{ label }}</Text>
    <slot />
  </View>
</template>
