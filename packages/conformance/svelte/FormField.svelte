<script lang="ts">
  import { Text, View } from '@inkoxide/svelte';
  import type { Snippet } from 'svelte';

  let { label, children }: { label: string; children: Snippet } = $props();
</script>

<View class="flex-1 gap-1">
  <Text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-500">{label}</Text>
  {@render children()}
</View>
