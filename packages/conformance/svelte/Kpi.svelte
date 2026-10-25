<script lang="ts">
  import { Text, View } from '@inkoxide/svelte';

  let { label, value, note }: { label: string; value: string; note: string } = $props();
</script>

<View class="flex-1 rounded-lg border border-gray-200 p-4">
  <Text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-500">{label}</Text>
  <Text class="mt-1 text-xl font-bold">{value}</Text>
  <Text class="text-emerald-600">{note}</Text>
</View>
