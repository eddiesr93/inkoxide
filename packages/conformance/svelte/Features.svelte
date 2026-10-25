<script lang="ts">
  import type { Painter } from '@inkoxide/svelte';
  import { Canvas, Checkbox, Circle, Document, Footer, Image, Link, Note, Page, Svg, Text, TextField, Tspan, View } from '@inkoxide/svelte';

  let { title, items }: { title: string; items: string[] } = $props();

  const paint = (p: Painter, w: number, h: number) => {
    p.rect(0, 0, w, h).fill('#fde68a');
  };
</script>

<Document {title}>
  <Page size="A5" style={{ padding: 30 }}>
    <Footer>
      <Text>
        {#snippet render(page)}Page {page.pageNumber} of {page.totalPages}{/snippet}
      </Text>
    </Footer>
    <Text style={{ fontSize: 12, '@media (min-width: 400pt)': { fontSize: 18 } }}>{title}</Text>
    <View class="flex-row gap-2">
      {#each items as item, i (item)}
        <Text class={i === 0 ? 'font-bold' : 'text-gray-500'}>{item}</Text>
      {/each}
    </View>
    <Text>A <Link href="https://example.com">link</Link> and a note <Note>Read me</Note>.</Text>
    <Image src="logo" style={{ width: 24, height: 24 }} />
    <Svg width={60} height={30} viewBox="0 0 60 30">
      <Circle cx={15} cy={15} r={10} fill="#0ea5e9" />
      <Text x={30} y={20} fontSize={10}>S<Tspan fill="red">vg</Tspan></Text>
    </Svg>
    <Canvas width={40} height={20} {paint} />
    <View class="flex-row gap-2">
      <TextField name="name" value="Ada" class="h-5 w-24 border border-gray-300" />
      <Checkbox name="agree" checked class="h-3 w-3 border border-gray-400" />
    </View>
  </Page>
</Document>
