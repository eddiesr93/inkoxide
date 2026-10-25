<script setup lang="ts">
import {
  Cell,
  Document,
  Footer,
  Header,
  Page,
  PageNumber,
  Rect,
  Row,
  Svg,
  Table,
  Text,
  TotalPages,
  View,
} from '@inkoxide/vue';

import Kpi from './Kpi.vue';

interface Sale {
  region: string;
  manager: string;
  units: number;
  revenue: number;
  growth: number;
}

const props = defineProps<{
  quarter: string;
  monthly: { month: string; revenue: number }[];
  sales: Sale[];
}>();

const usd = (n: number) =>
  n.toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 0 });
const revenue = props.sales.reduce((sum, s) => sum + s.revenue, 0);
const units = props.sales.reduce((sum, s) => sum + s.units, 0);
const th =
  'border-b border-gray-300 bg-gray-50 px-2 py-2 text-[7.5pt] font-bold uppercase tracking-wider text-gray-500';
const td = 'border-b border-gray-100 px-2 py-1.5';
const max = Math.max(...props.monthly.map((d) => d.revenue));
const w = 499;
const h = 120;
const bar = w / props.monthly.length;
</script>

<template>
  <Document :title="`Sales report ${quarter}`" author="Acme Studio">
    <Page size="A4" class="p-16 text-[8.5pt] text-gray-900">
      <Header class="flex-row justify-between text-[7.5pt] text-gray-400">
        <Text>Acme Studio</Text>
        <Text pages="notFirst">Sales report · {{ quarter }}</Text>
      </Header>
      <Footer class="flex-row justify-between text-[7.5pt] text-gray-400">
        <Text>Confidential</Text>
        <Text class="text-right">Page <PageNumber /> of <TotalPages /></Text>
      </Footer>

      <Text class="text-3xl font-bold tracking-tight">Sales report, {{ quarter }}</Text>
      <Text class="mb-8 text-sm text-gray-500">Revenue by region, with the month-by-month trend.</Text>

      <View class="mb-8 flex-row gap-3">
        <Kpi label="Revenue" :value="usd(revenue)" note="+12.4% vs Q2" />
        <Kpi label="Units sold" :value="units.toLocaleString('en-US')" note="+8.1% vs Q2" />
        <Kpi label="Regions" :value="String(sales.length)" note="3 new this quarter" />
      </View>

      <Text class="mb-3 text-base font-bold" bookmark="Monthly revenue">Monthly revenue</Text>
      <View class="mb-8">
        <Svg :width="w" :height="h" :view-box="`0 0 ${w} ${h}`">
          <Rect
            v-for="(d, i) in monthly"
            :key="d.month"
            :x="i * bar + 6"
            :y="h - (d.revenue / max) * (h - 8)"
            :width="bar - 12"
            :height="(d.revenue / max) * (h - 8)"
            :rx="3"
            :fill="i === monthly.length - 1 ? '#4f46e5' : '#c7d2fe'"
          />
        </Svg>
        <View class="mt-1 flex-row" :style="{ width: w }">
          <Text v-for="d in monthly" :key="d.month" class="flex-1 text-center text-[7.5pt] text-gray-500">
            {{ d.month }}
          </Text>
        </View>
      </View>

      <Text class="mb-3 text-base font-bold" bookmark="By region" :break="{ minPresenceAhead: 120 }"
        >By region</Text
      >
      <Table :columns="['1fr', '1fr', 50, 80, 50]" :header-rows="1">
        <Row>
          <Cell :class="th">Region</Cell>
          <Cell :class="th">Manager</Cell>
          <Cell :class="`${th} text-right`">Units</Cell>
          <Cell :class="`${th} text-right`">Revenue</Cell>
          <Cell :class="`${th} text-right`">Growth</Cell>
        </Row>
        <Row v-for="s in sales" :key="s.region">
          <Cell :class="`${td} font-bold`">{{ s.region }}</Cell>
          <Cell :class="`${td} text-gray-600`">{{ s.manager }}</Cell>
          <Cell :class="`${td} text-right`">{{ s.units }}</Cell>
          <Cell :class="`${td} text-right`">{{ usd(s.revenue) }}</Cell>
          <Cell :class="`${td} text-right ${s.growth < 0 ? 'text-rose-600' : 'text-emerald-600'}`"
            >{{ s.growth > 0 ? '+' : '' }}{{ s.growth.toFixed(1) }}%</Cell
          >
        </Row>
      </Table>
    </Page>
  </Document>
</template>
