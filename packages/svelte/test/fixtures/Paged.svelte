<script lang="ts">
  import { setContext } from 'svelte';

  import { Document, Page, Text } from '../../src/components/index.ts';
  import Num from './Num.svelte';

  setContext('who', 'p.');
</script>

<Document><Page><Text>{#snippet render()}<Num />{/snippet}</Text></Page></Document>
