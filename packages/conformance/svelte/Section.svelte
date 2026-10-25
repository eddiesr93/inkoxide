<script lang="ts">
  import { Text, View } from '@inkoxide/svelte';
  import type { Snippet } from 'svelte';

  let { title, children }: { title: string; children: Snippet } = $props();
</script>

<View class="mb-6">
  <Text class="mb-3 text-[7.5pt] font-bold uppercase tracking-widest text-sky-700">{title}</Text>
  {@render children()}
</View>
