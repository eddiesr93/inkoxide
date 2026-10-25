<script lang="ts">
  import { Document, Page, Text } from '@inkoxide/svelte';

  import Inner from './Inner.svelte';
</script>

<Document><Page><Text>Hi <Inner /></Text></Page></Document>
