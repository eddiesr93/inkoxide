<script lang="ts">
  import { getContext } from 'svelte';

  import { usePage } from '../../src/page.ts';

  const page = usePage();
  const who = getContext<string>('who');
</script>

{who} {page.pageNumber}
