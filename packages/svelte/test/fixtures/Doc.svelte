<script lang="ts">
  import { Document, Footer, Link, Page, Text, TextField, View } from '../../src/components/index.ts';

  let { name, lines = ['a', 'b'] }: { name: string; lines?: string[] } = $props();
</script>

<Document title="T">
  <Page size="A5" style={{ padding: 20 }}>
    <View class="flex-row gap-2">
      <Text hyphenate lang="en" hyphenationPenalty={900}>Hello {name} <Link href="https://x.y">link</Link> end</Text>
    </View>
    {#each lines as line (line)}
      <Text class="font-bold">{line}</Text>
    {/each}
    <TextField name="email" />
    <Footer>
      <Text>
        {#snippet render(page)}{page.pageNumber} / {page.totalPages}{/snippet}
      </Text>
    </Footer>
  </Page>
</Document>
