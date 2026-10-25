<script lang="ts">
  import { Document, Page, Text, View } from '@inkoxide/svelte';

  import Section from './Section.svelte';

  let {
    name,
    title,
    contact,
    summary,
    skills,
    jobs,
    education,
  }: {
    name: string;
    title: string;
    contact: string[];
    summary: string;
    skills: { group: string; items: string[] }[];
    jobs: { role: string; company: string; period: string; points: string[] }[];
    education: { degree: string; school: string; year: string }[];
  } = $props();

  const initials = $derived(
    name
      .split(' ')
      .map((n) => n[0])
      .join(''),
  );
</script>

<Document title={`${name} — Résumé`} author={name}>
  <Page size="A4" class="text-[8.5pt] leading-snug text-slate-800">
    <View class="grid flex-1 grid-cols-[170pt_1fr]">
      <View class="bg-slate-900 px-8 py-12 text-slate-300">
        <View class="mb-5 h-16 w-16 items-center justify-center rounded-full bg-sky-500">
          <Text class="text-xl font-bold text-white">{initials}</Text>
        </View>
        <Text class="text-xl font-bold leading-tight text-white">{name}</Text>
        <Text class="mb-8 text-sky-300">{title}</Text>
        <Text class="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">Contact</Text>
        <View class="mb-8 gap-1">
          {#each contact as c (c)}<Text>{c}</Text>{/each}
        </View>
        {#each skills as s (s.group)}
          <View class="mb-5">
            <Text class="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">{s.group}</Text>
            <View class="flex-row flex-wrap gap-1">
              {#each s.items as it (it)}
                <Text class="rounded bg-slate-800 px-1.5 py-0.5 text-[7.5pt] text-slate-200">{it}</Text>
              {/each}
            </View>
          </View>
        {/each}
      </View>
      <View class="px-10 py-12">
        <Section title="Profile">
          <Text class="text-[9pt] leading-relaxed text-slate-600">{summary}</Text>
        </Section>
        <Section title="Experience">
          {#each jobs as j (j.company)}
            <View class="mb-4" break="avoid">
              <View class="flex-row items-baseline justify-between">
                <Text class="text-[10pt] font-bold text-slate-900">{j.role}</Text>
                <Text class="text-slate-400">{j.period}</Text>
              </View>
              <Text class="mb-1.5 text-sky-700">{j.company}</Text>
              {#each j.points as p (p)}
                <View class="mb-0.5 flex-row gap-2">
                  <Text class="text-sky-500">•</Text>
                  <Text class="flex-1 text-slate-600">{p}</Text>
                </View>
              {/each}
            </View>
          {/each}
        </Section>
        <Section title="Education">
          {#each education as e (e.degree)}
            <View class="flex-row justify-between">
              <View>
                <Text class="font-bold text-slate-900">{e.degree}</Text>
                <Text class="text-slate-500">{e.school}</Text>
              </View>
              <Text class="text-slate-400">{e.year}</Text>
            </View>
          {/each}
        </Section>
      </View>
    </View>
  </Page>
</Document>
