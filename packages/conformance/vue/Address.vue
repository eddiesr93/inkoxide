<script setup lang="ts">
import { Text, View } from '@inkoxide/vue';

defineProps<{ label: string; party: { name: string; address: string[]; email: string } }>();
</script>

<template>
  <View class="flex-1">
    <Text class="mb-1 text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">{{ label }}</Text>
    <Text class="font-bold">{{ party.name }}</Text>
    <Text v-for="line in party.address" :key="line" class="text-gray-600">{{ line }}</Text>
    <Text class="text-gray-600">{{ party.email }}</Text>
  </View>
</template>
