<script lang="ts">
  import { Text, View } from '@inkoxide/svelte';

  let { label, party }: { label: string; party: { name: string; address: string[]; email: string } } = $props();
</script>

<View class="flex-1">
  <Text class="mb-1 text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">{label}</Text>
  <Text class="font-bold">{party.name}</Text>
  {#each party.address as line (line)}
    <Text class="text-gray-600">{line}</Text>
  {/each}
  <Text class="text-gray-600">{party.email}</Text>
</View>
