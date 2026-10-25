<script lang="ts">
  import { Text } from '@inkoxide/svelte';
  import { onDestroy } from 'svelte';

  import { live } from '../live.ts';

  live.created++;
  onDestroy(() => live.released++);
</script>

<Text>Body</Text>
