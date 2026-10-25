import { realpathSync } from 'node:fs';

import { Document, Page, renderToFile, Text, View } from '@inkoxide/react';

interface Job {
  role: string;
  company: string;
  period: string;
  points: string[];
}

export interface ResumeProps {
  name: string;
  title: string;
  contact: string[];
  summary: string;
  skills: { group: string; items: string[] }[];
  jobs: Job[];
  education: { degree: string; school: string; year: string }[];
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <View className="mb-6">
      <Text className="mb-3 text-[7.5pt] font-bold uppercase tracking-widest text-sky-700">{title}</Text>
      {children}
    </View>
  );
}

export default function Resume({ name, title, contact, summary, skills, jobs, education }: ResumeProps) {
  return (
    <Document title={`${name} — Résumé`} author={name}>
      <Page size="A4" className="text-[8.5pt] leading-snug text-slate-800">
        <View className="grid flex-1 grid-cols-[170pt_1fr]">
          <View className="bg-slate-900 px-8 py-12 text-slate-300">
            <View className="mb-5 h-16 w-16 items-center justify-center rounded-full bg-sky-500">
              <Text className="text-xl font-bold text-white">
                {name
                  .split(' ')
                  .map((n) => n[0])
                  .join('')}
              </Text>
            </View>
            <Text className="text-xl font-bold leading-tight text-white">{name}</Text>
            <Text className="mb-8 text-sky-300">{title}</Text>

            <Text className="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">
              Contact
            </Text>
            <View className="mb-8 gap-1">
              {contact.map((c) => (
                <Text key={c}>{c}</Text>
              ))}
            </View>

            {skills.map((s) => (
              <View key={s.group} className="mb-5">
                <Text className="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">
                  {s.group}
                </Text>
                <View className="flex-row flex-wrap gap-1">
                  {s.items.map((it) => (
                    <Text key={it} className="rounded bg-slate-800 px-1.5 py-0.5 text-[7.5pt] text-slate-200">
                      {it}
                    </Text>
                  ))}
                </View>
              </View>
            ))}
          </View>

          <View className="px-10 py-12">
            <Section title="Profile">
              <Text className="text-[9pt] leading-relaxed text-slate-600">{summary}</Text>
            </Section>

            <Section title="Experience">
              {jobs.map((j) => (
                <View key={j.company} className="mb-4" break="avoid">
                  <View className="flex-row items-baseline justify-between">
                    <Text className="text-[10pt] font-bold text-slate-900">{j.role}</Text>
                    <Text className="text-slate-400">{j.period}</Text>
                  </View>
                  <Text className="mb-1.5 text-sky-700">{j.company}</Text>
                  {j.points.map((p) => (
                    <View key={p} className="mb-0.5 flex-row gap-2">
                      <Text className="text-sky-500">•</Text>
                      <Text className="flex-1 text-slate-600">{p}</Text>
                    </View>
                  ))}
                </View>
              ))}
            </Section>

            <Section title="Education">
              {education.map((e) => (
                <View key={e.degree} className="flex-row justify-between">
                  <View>
                    <Text className="font-bold text-slate-900">{e.degree}</Text>
                    <Text className="text-slate-500">{e.school}</Text>
                  </View>
                  <Text className="text-slate-400">{e.year}</Text>
                </View>
              ))}
            </Section>
          </View>
        </View>
      </Page>
    </Document>
  );
}

export const sample: ResumeProps = {
  name: 'Grace Hopper',
  title: 'Principal Software Engineer',
  contact: ['grace@example.com', '+1 (555) 010-1906', 'Arlington, VA', 'github.com/ghopper'],
  summary:
    'Engineer with twenty years of experience building compilers, developer tools and the teams that ship them. I care about making machines speak the language of the people who use them, and about documentation that is as carefully made as the code.',
  skills: [
    { group: 'Languages', items: ['TypeScript', 'Rust', 'COBOL', 'Go', 'SQL'] },
    { group: 'Systems', items: ['Compilers', 'Distributed systems', 'PostgreSQL', 'Kubernetes'] },
    { group: 'Leadership', items: ['Mentoring', 'Hiring', 'Technical writing'] },
  ],
  jobs: [
    {
      role: 'Principal Engineer',
      company: 'Compiler Systems Inc.',
      period: '2019 — now',
      points: [
        'Led the rewrite of the code generator, cutting build times by 60% across 400 services.',
        'Designed the plugin API now used by eleven internal teams.',
        'Mentored eight engineers, four of whom became tech leads.',
      ],
    },
    {
      role: 'Senior Engineer',
      company: 'Eckert-Mauchly Computers',
      period: '2013 — 2019',
      points: [
        'Built the first English-like query language for the reporting platform.',
        'Introduced code review and automated testing to a team of thirty.',
      ],
    },
    {
      role: 'Software Engineer',
      company: 'Harvard Computation Lab',
      period: '2008 — 2013',
      points: [
        'Maintained the operating manuals and the subroutine library.',
        'Found and documented the first actual bug.',
      ],
    },
  ],
  education: [
    { degree: 'Ph.D., Mathematics', school: 'Yale University', year: '2008' },
    { degree: 'B.A., Mathematics and Physics', school: 'Vassar College', year: '2003' },
  ],
};

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename)
  await renderToFile(<Resume {...sample} />, 'resume.pdf');
