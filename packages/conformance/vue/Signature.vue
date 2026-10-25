<script setup lang="ts">
import { Text, View } from '@inkoxide/vue';

defineProps<{ name: string; caption: string }>();
</script>

<template>
  <View class="w-48 items-center">
    <View class="mb-2 w-full border-b border-stone-400" />
    <Text class="font-bold">{{ name }}</Text>
    <Text class="text-stone-500">{{ caption }}</Text>
  </View>
</template>
