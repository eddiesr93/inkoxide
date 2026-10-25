<script lang="ts">
  import { Document, Page, PageBreak, Text } from '@inkoxide/svelte';

  import Counted from './Counted.svelte';
  import PageLabel from './PageLabel.svelte';
</script>

<Document>
  <Page size="A6">
    <Text>
      {#snippet render()}<PageLabel />{/snippet}
    </Text>
    <Counted />
    <PageBreak />
    <Text>More</Text>
  </Page>
</Document>
