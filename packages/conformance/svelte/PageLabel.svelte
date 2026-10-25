<script lang="ts">
  import { usePage } from '@inkoxide/svelte';
  import { onDestroy } from 'svelte';

  import { live } from '../live.ts';

  const page = usePage();
  live.created++;
  onDestroy(() => live.released++);
</script>

Page {page.pageNumber} of {page.totalPages}
