<script lang="ts">
  // The root of a `render` snippet's content for one page: it gives the page to `usePage()`.
  import type { PageInfo } from '@inkoxide/tree';
  import { setContext, type Snippet } from 'svelte';

  import { PAGE } from '../capture.ts';

  let { snippet, page }: { snippet: Snippet<[PageInfo]>; page: PageInfo } = $props();
  // A template renders once: the page does not change.
  // svelte-ignore state_referenced_locally
  setContext(PAGE, page);
</script>

{@render snippet(page)}
