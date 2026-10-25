// The examples of examples/ as Angular template components, compiled just in time: decorated
// inputs, since signal inputs need the AOT compiler.
import { Component, CUSTOM_ELEMENTS_SCHEMA, Input } from '@angular/core';

const money = (n: number) => n.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
const percent = (n: number) => n.toLocaleString('en-US', { style: 'percent', maximumFractionDigits: 2 });
const usd = (n: number) =>
  n.toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 0 });

interface Party {
  name: string;
  address: string[];
  email: string;
}

@Component({
  selector: 'app-receipt',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="'Receipt for ' + customer">
      <ink-page size="A6" class="p-8 text-xs text-gray-900">
        <ink-text class="text-lg font-bold">Receipt</ink-text>
        <ink-text class="mb-4 text-gray-500">for {{ customer }}</ink-text>
        @for (l of lines; track l.name) {
          <ink-view class="flex-row justify-between border-b border-gray-200 py-1.5">
            <ink-text>{{ l.name }}</ink-text>
            <ink-text>\${{ l.amount.toFixed(2) }}</ink-text>
          </ink-view>
        }
        <ink-view class="mt-3 flex-row justify-between font-bold">
          <ink-text>Total</ink-text>
          <ink-text>\${{ total().toFixed(2) }}</ink-text>
        </ink-view>
      </ink-page>
    </ink-document>
  `,
})
export class Receipt {
  @Input() customer = '';
  @Input() lines: { name: string; amount: number }[] = [];
  total = () => this.lines.reduce((sum, l) => sum + l.amount, 0);
}

@Component({
  selector: 'app-letter',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document title="Letter" [author]="sender.name" lang="en-US">
      <ink-page size="Letter" class="px-24 pt-20 pb-24 text-[10.5pt] leading-relaxed text-slate-800">
        <ink-footer class="text-[7.5pt] text-slate-400">
          <ink-view class="flex-row justify-between border-t border-slate-200 pt-3">
            <ink-text>Northwind Labs · 500 Market Street, San Francisco, CA 94105</ink-text>
            <ink-text href="mailto:hello@northwind.dev">hello&#64;northwind.dev</ink-text>
          </ink-view>
        </ink-footer>
        <ink-view class="mb-14 flex-row items-center gap-2">
          <ink-view class="h-3 w-3 rotate-45 bg-teal-600" />
          <ink-text class="text-base font-bold tracking-tight text-slate-900">Northwind Labs</ink-text>
        </ink-view>
        <ink-text class="mb-8 text-slate-500">{{ date }}</ink-text>
        <ink-view class="mb-8">
          @for (line of to; track line) {
            <ink-text>{{ line }}</ink-text>
          }
        </ink-view>
        <ink-text class="mb-4">{{ greeting }}</ink-text>
        @for (p of paragraphs; track p) {
          <ink-text class="mb-3 text-justify" hyphenate>{{ p }}</ink-text>
        }
        <ink-view class="mt-6" break="avoid">
          <ink-text>Kind regards,</ink-text>
          <ink-text class="mt-10 font-bold text-slate-900">{{ sender.name }}</ink-text>
          <ink-text class="text-slate-500">{{ sender.title }}</ink-text>
        </ink-view>
      </ink-page>
    </ink-document>
  `,
})
export class Letter {
  @Input() date = '';
  @Input() to: string[] = [];
  @Input() greeting = '';
  @Input() paragraphs: string[] = [];
  @Input() sender = { name: '', title: '' };
}

@Component({
  selector: 'app-address',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-view class="flex-1">
      <ink-text class="mb-1 text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">{{
        label
      }}</ink-text>
      <ink-text class="font-bold">{{ party.name }}</ink-text>
      @for (line of party.address; track line) {
        <ink-text class="text-gray-600">{{ line }}</ink-text>
      }
      <ink-text class="text-gray-600">{{ party.email }}</ink-text>
    </ink-view>
  `,
})
export class Address {
  @Input() label = '';
  @Input() party!: Party;
}

@Component({
  selector: 'app-invoice',
  imports: [Address],
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="'Invoice ' + number" [author]="from.name">
      <ink-page size="A4" class="px-16 pt-14 pb-16 text-[9pt] text-gray-900">
        <ink-footer class="flex-row justify-between text-[7.5pt] text-gray-400">
          <ink-text>{{ from.name }} · {{ from.email }}</ink-text>
          <ink-text class="text-right">Page <ink-page-number /> of <ink-total-pages /></ink-text>
        </ink-footer>

        <ink-view class="mb-12 flex-row items-start justify-between">
          <ink-view class="flex-row items-center gap-2.5">
            <ink-svg [width]="28" [height]="28" viewBox="0 0 32 32">
              <ink-circle [cx]="16" [cy]="16" [r]="16" fill="#4f46e5" />
              <ink-path
                d="M10 21 L16 9 L22 21 M12.5 16.5 H19.5"
                fill="none"
                stroke="white"
                [strokeWidth]="2.4"
                strokeLinecap="round"
              />
            </ink-svg>
            <ink-text class="text-base font-bold">{{ from.name }}</ink-text>
          </ink-view>
          <ink-view class="items-end">
            <ink-text class="text-3xl font-bold tracking-tight text-indigo-600">Invoice</ink-text>
            <ink-text class="text-gray-500">#{{ number }}</ink-text>
          </ink-view>
        </ink-view>

        <ink-view class="mb-10 flex-row gap-8">
          <app-address label="From" [party]="from" />
          <app-address label="Bill to" [party]="to" />
          <ink-view class="w-32 gap-3">
            <ink-view>
              <ink-text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-400"
                >Issued</ink-text
              >
              <ink-text>{{ issued }}</ink-text>
            </ink-view>
            <ink-view>
              <ink-text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-400">Due</ink-text>
              <ink-text class="font-bold">{{ due }}</ink-text>
            </ink-view>
          </ink-view>
        </ink-view>

        <ink-table [columns]="['1fr', 50, 80, 90]" [headerRows]="1">
          <ink-row>
            <ink-cell [class]="th">Description</ink-cell>
            <ink-cell [class]="th + ' text-right'">Qty</ink-cell>
            <ink-cell [class]="th + ' text-right'">Price</ink-cell>
            <ink-cell [class]="th + ' text-right'">Amount</ink-cell>
          </ink-row>
          @for (it of items; track it.description) {
            <ink-row>
              <ink-cell [class]="td">
                <ink-text class="font-bold">{{ it.description }}</ink-text>
                @if (it.details) {
                  <ink-text class="text-gray-500">{{ it.details }}</ink-text>
                }
              </ink-cell>
              <ink-cell [class]="td + ' text-right'">{{ it.quantity }}</ink-cell>
              <ink-cell [class]="td + ' text-right'">{{ money(it.price) }}</ink-cell>
              <ink-cell [class]="td + ' text-right font-bold'">{{ money(it.quantity * it.price) }}</ink-cell>
            </ink-row>
          }
        </ink-table>

        <ink-view class="mt-6 ml-auto w-56 gap-1.5" break="avoid">
          <ink-view class="flex-row justify-between text-gray-600">
            <ink-text>Subtotal</ink-text>
            <ink-text>{{ money(subtotal()) }}</ink-text>
          </ink-view>
          <ink-view class="flex-row justify-between text-gray-600">
            <ink-text>Tax ({{ percent(taxRate) }})</ink-text>
            <ink-text>{{ money(subtotal() * taxRate) }}</ink-text>
          </ink-view>
          <ink-view
            class="mt-1 flex-row justify-between rounded-md bg-indigo-50 px-3 py-2.5 text-sm font-bold text-indigo-700"
          >
            <ink-text>Total due</ink-text>
            <ink-text>{{ money(subtotal() + subtotal() * taxRate) }}</ink-text>
          </ink-view>
        </ink-view>

        <ink-view class="mt-12 rounded-md bg-gray-50 p-4 text-gray-600" break="avoid">
          <ink-text class="font-bold text-gray-900">Payment</ink-text>
          <ink-text>Bank transfer within 30 days. Please quote #{{ number }} with your payment.</ink-text>
        </ink-view>
      </ink-page>
    </ink-document>
  `,
})
export class Invoice {
  @Input() number = '';
  @Input() issued = '';
  @Input() due = '';
  @Input() from!: Party;
  @Input() to!: Party;
  @Input() items: { description: string; details?: string; quantity: number; price: number }[] = [];
  @Input() taxRate = 0;
  readonly money = money;
  readonly percent = percent;
  readonly th = 'border-b border-gray-300 py-2 text-[7.5pt] font-bold uppercase tracking-wider text-gray-500';
  readonly td = 'border-b border-gray-100 py-2.5';
  subtotal = () => this.items.reduce((sum, it) => sum + it.quantity * it.price, 0);
}

@Component({
  selector: 'app-form-field',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-view class="flex-1 gap-1">
      <ink-text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-500">{{ label }}</ink-text>
      <ng-content />
    </ink-view>
  `,
})
export class FormField {
  @Input() label = '';
}

@Component({
  selector: 'app-form',
  imports: [FormField],
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="title">
      <ink-page size="A4" class="p-16 text-[9pt] text-gray-900">
        <ink-view class="mb-8 border-b-2 border-rose-500 pb-4">
          <ink-text class="text-2xl font-bold">{{ title }}</ink-text>
          <ink-text class="text-gray-500"
            >Fill this form in your PDF viewer, then save and send it back.</ink-text
          >
        </ink-view>

        <ink-text class="mb-3 text-sm font-bold">Your details</ink-text>
        <ink-view class="mb-4 flex-row gap-4">
          <app-form-field label="First name"
            ><ink-text-field name="firstName" [class]="input"
          /></app-form-field>
          <app-form-field label="Last name"
            ><ink-text-field name="lastName" [class]="input"
          /></app-form-field>
        </ink-view>
        <ink-view class="mb-4 flex-row gap-4">
          <app-form-field label="Email"><ink-text-field name="email" [class]="input" /></app-form-field>
          <app-form-field label="Plan"
            ><ink-select name="plan" [options]="plans" [value]="plans[0]" [class]="input"
          /></app-form-field>
        </ink-view>
        <ink-view class="mb-8">
          <app-form-field label="Anything we should know?">
            <ink-text-field
              name="notes"
              multiline
              class="h-20 w-full rounded border border-gray-300 bg-gray-50 p-1 text-[9pt]"
            />
          </app-form-field>
        </ink-view>

        <ink-text class="mb-3 text-sm font-bold">Billing</ink-text>
        <ink-view class="mb-8 flex-row gap-6">
          @for (period of periods; track period) {
            <ink-view class="flex-row items-center gap-2">
              <ink-radio
                name="billing"
                [value]="period"
                [checked]="period === 'Yearly'"
                class="h-3.5 w-3.5 rounded-full border border-gray-400"
              />
              <ink-text>{{ period }}</ink-text>
            </ink-view>
          }
        </ink-view>

        <ink-view class="gap-2 rounded-lg bg-rose-50 p-4">
          @for (label of consents; track label; let i = $index) {
            <ink-view class="flex-row items-center gap-2">
              <ink-checkbox [name]="'consent' + i" class="h-3.5 w-3.5 border border-gray-400 bg-white" />
              <ink-text>{{ label }}</ink-text>
            </ink-view>
          }
        </ink-view>
      </ink-page>
    </ink-document>
  `,
})
export class Form {
  @Input() title = '';
  @Input() plans: string[] = [];
  readonly input = 'h-6 w-full rounded border border-gray-300 bg-gray-50 px-1 text-[9pt]';
  readonly periods = ['Monthly', 'Yearly'];
  readonly consents = ['I agree to the terms of service.', 'Send me product news, at most once a month.'];
}

@Component({
  selector: 'app-signature',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-view class="w-48 items-center">
      <ink-view class="mb-2 w-full border-b border-stone-400" />
      <ink-text class="font-bold">{{ name }}</ink-text>
      <ink-text class="text-stone-500">{{ caption }}</ink-text>
    </ink-view>
  `,
})
export class Signature {
  @Input() name = '';
  @Input() caption = '';
}

@Component({
  selector: 'app-certificate',
  imports: [Signature],
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="'Certificate: ' + recipient">
      <ink-page size="A4-landscape" class="bg-amber-50 p-6 text-[9pt] text-stone-800">
        <ink-view class="flex-1 border-2 border-amber-700 p-1.5">
          <ink-view class="flex-1 items-center border border-amber-700/50 px-20 py-12">
            <ink-text class="text-[8pt] font-bold uppercase tracking-[0.4em] text-amber-700"
              >Acme Academy</ink-text
            >
            <ink-view class="flex-1 items-center justify-center">
              <ink-text class="font-display text-[40pt] text-stone-900">Certificate of Completion</ink-text>
              <ink-text class="mt-4 text-sm text-stone-500">This certifies that</ink-text>
              <ink-text
                class="mt-4 border-b border-amber-700/40 px-10 pb-2 font-display text-[30pt] text-amber-800"
              >
                {{ recipient }}
              </ink-text>
              <ink-text class="mt-5 max-w-md text-center text-sm leading-relaxed text-stone-600">
                has completed <ink-text class="font-bold text-stone-900">{{ course }}</ink-text
                >, a {{ hours }}-hour course with practical assessments, and is awarded this certificate.
              </ink-text>
            </ink-view>
            <ink-view class="w-full flex-row items-end justify-between">
              <app-signature name="Grace Hopper" caption="Course director" />
              <ink-svg [width]="84" [height]="84" viewBox="0 0 100 100">
                <ink-path [d]="seal" fill="#b45309" />
                <ink-circle [cx]="50" [cy]="50" [r]="34" fill="#f59e0b" />
                <ink-circle [cx]="50" [cy]="50" [r]="28" fill="none" stroke="#fef3c7" [strokeWidth]="1.5" />
                <ink-path
                  d="M38 51 L46 59 L63 41"
                  fill="none"
                  stroke="white"
                  [strokeWidth]="5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </ink-svg>
              <app-signature [name]="date" caption="Date of issue" />
            </ink-view>
          </ink-view>
        </ink-view>
        <ink-text class="absolute right-10 bottom-2 text-[6.5pt] text-stone-400"
          >Certificate {{ id }}</ink-text
        >
      </ink-page>
    </ink-document>
  `,
})
export class Certificate {
  @Input() recipient = '';
  @Input() course = '';
  @Input() hours = 0;
  @Input() date = '';
  @Input() id = '';
  readonly seal = `M${Array.from({ length: 24 }, (_, i) => {
    const a = (i * Math.PI) / 12;
    const r = i % 2 === 0 ? 46 : 40;
    return `${50 + r * Math.cos(a)},${50 + r * Math.sin(a)}`;
  }).join(' L')} Z`;
}

@Component({
  selector: 'app-kpi',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-view class="flex-1 rounded-lg border border-gray-200 p-4">
      <ink-text class="text-[7.5pt] font-bold uppercase tracking-wider text-gray-500">{{ label }}</ink-text>
      <ink-text class="mt-1 text-xl font-bold">{{ value }}</ink-text>
      <ink-text class="text-emerald-600">{{ note }}</ink-text>
    </ink-view>
  `,
})
export class Kpi {
  @Input() label = '';
  @Input() value = '';
  @Input() note = '';
}

interface Sale {
  region: string;
  manager: string;
  units: number;
  revenue: number;
  growth: number;
}

@Component({
  selector: 'app-report',
  imports: [Kpi],
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="'Sales report ' + quarter" author="Acme Studio">
      <ink-page size="A4" class="p-16 text-[8.5pt] text-gray-900">
        <ink-header class="flex-row justify-between text-[7.5pt] text-gray-400">
          <ink-text>Acme Studio</ink-text>
          <ink-text pages="notFirst">Sales report · {{ quarter }}</ink-text>
        </ink-header>
        <ink-footer class="flex-row justify-between text-[7.5pt] text-gray-400">
          <ink-text>Confidential</ink-text>
          <ink-text class="text-right">Page <ink-page-number /> of <ink-total-pages /></ink-text>
        </ink-footer>

        <ink-text class="text-3xl font-bold tracking-tight">Sales report, {{ quarter }}</ink-text>
        <ink-text class="mb-8 text-sm text-gray-500"
          >Revenue by region, with the month-by-month trend.</ink-text
        >

        <ink-view class="mb-8 flex-row gap-3">
          <app-kpi label="Revenue" [value]="usd(revenue())" note="+12.4% vs Q2" />
          <app-kpi label="Units sold" [value]="units().toLocaleString('en-US')" note="+8.1% vs Q2" />
          <app-kpi label="Regions" [value]="'' + sales.length" note="3 new this quarter" />
        </ink-view>

        <ink-text class="mb-3 text-base font-bold" bookmark="Monthly revenue">Monthly revenue</ink-text>
        <ink-view class="mb-8">
          <ink-svg [width]="w" [height]="h" [viewBox]="'0 0 ' + w + ' ' + h">
            @for (d of monthly; track d.month; let i = $index, last = $last) {
              <ink-rect
                [x]="i * bar() + 6"
                [y]="h - (d.revenue / max()) * (h - 8)"
                [width]="bar() - 12"
                [height]="(d.revenue / max()) * (h - 8)"
                [rx]="3"
                [fill]="last ? '#4f46e5' : '#c7d2fe'"
              />
            }
          </ink-svg>
          <ink-view class="mt-1 flex-row" [style]="{ width: w }">
            @for (d of monthly; track d.month) {
              <ink-text class="flex-1 text-center text-[7.5pt] text-gray-500">{{ d.month }}</ink-text>
            }
          </ink-view>
        </ink-view>

        <ink-text class="mb-3 text-base font-bold" bookmark="By region" [break]="{ minPresenceAhead: 120 }"
          >By region</ink-text
        >
        <ink-table [columns]="['1fr', '1fr', 50, 80, 50]" [headerRows]="1">
          <ink-row>
            <ink-cell [class]="th">Region</ink-cell>
            <ink-cell [class]="th">Manager</ink-cell>
            <ink-cell [class]="th + ' text-right'">Units</ink-cell>
            <ink-cell [class]="th + ' text-right'">Revenue</ink-cell>
            <ink-cell [class]="th + ' text-right'">Growth</ink-cell>
          </ink-row>
          @for (s of sales; track s.region) {
            <ink-row>
              <ink-cell [class]="td + ' font-bold'">{{ s.region }}</ink-cell>
              <ink-cell [class]="td + ' text-gray-600'">{{ s.manager }}</ink-cell>
              <ink-cell [class]="td + ' text-right'">{{ s.units }}</ink-cell>
              <ink-cell [class]="td + ' text-right'">{{ usd(s.revenue) }}</ink-cell>
              <ink-cell [class]="td + ' text-right ' + (s.growth < 0 ? 'text-rose-600' : 'text-emerald-600')"
                >{{ s.growth > 0 ? '+' : '' }}{{ s.growth.toFixed(1) }}%</ink-cell
              >
            </ink-row>
          }
        </ink-table>
      </ink-page>
    </ink-document>
  `,
})
export class Report {
  @Input() quarter = '';
  @Input() monthly: { month: string; revenue: number }[] = [];
  @Input() sales: Sale[] = [];
  readonly usd = usd;
  readonly w = 499;
  readonly h = 120;
  readonly th =
    'border-b border-gray-300 bg-gray-50 px-2 py-2 text-[7.5pt] font-bold uppercase tracking-wider text-gray-500';
  readonly td = 'border-b border-gray-100 px-2 py-1.5';
  revenue = () => this.sales.reduce((sum, s) => sum + s.revenue, 0);
  units = () => this.sales.reduce((sum, s) => sum + s.units, 0);
  max = () => Math.max(...this.monthly.map((d) => d.revenue));
  bar = () => this.w / this.monthly.length;
}

@Component({
  selector: 'app-section',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-view class="mb-6">
      <ink-text class="mb-3 text-[7.5pt] font-bold uppercase tracking-widest text-sky-700">{{
        title
      }}</ink-text>
      <ng-content />
    </ink-view>
  `,
})
export class Section {
  @Input() title = '';
}

@Component({
  selector: 'app-resume',
  imports: [Section],
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="name + ' — Résumé'" [author]="name">
      <ink-page size="A4" class="text-[8.5pt] leading-snug text-slate-800">
        <ink-view class="grid flex-1 grid-cols-[170pt_1fr]">
          <ink-view class="bg-slate-900 px-8 py-12 text-slate-300">
            <ink-view class="mb-5 h-16 w-16 items-center justify-center rounded-full bg-sky-500">
              <ink-text class="text-xl font-bold text-white">{{ initials() }}</ink-text>
            </ink-view>
            <ink-text class="text-xl font-bold leading-tight text-white">{{ name }}</ink-text>
            <ink-text class="mb-8 text-sky-300">{{ title }}</ink-text>
            <ink-text class="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400"
              >Contact</ink-text
            >
            <ink-view class="mb-8 gap-1">
              @for (c of contact; track c) {
                <ink-text>{{ c }}</ink-text>
              }
            </ink-view>
            @for (s of skills; track s.group) {
              <ink-view class="mb-5">
                <ink-text class="mb-2 text-[7.5pt] font-bold uppercase tracking-widest text-slate-400">{{
                  s.group
                }}</ink-text>
                <ink-view class="flex-row flex-wrap gap-1">
                  @for (it of s.items; track it) {
                    <ink-text class="rounded bg-slate-800 px-1.5 py-0.5 text-[7.5pt] text-slate-200">{{
                      it
                    }}</ink-text>
                  }
                </ink-view>
              </ink-view>
            }
          </ink-view>
          <ink-view class="px-10 py-12">
            <app-section title="Profile">
              <ink-text class="text-[9pt] leading-relaxed text-slate-600">{{ summary }}</ink-text>
            </app-section>
            <app-section title="Experience">
              @for (j of jobs; track j.company) {
                <ink-view class="mb-4" break="avoid">
                  <ink-view class="flex-row items-baseline justify-between">
                    <ink-text class="text-[10pt] font-bold text-slate-900">{{ j.role }}</ink-text>
                    <ink-text class="text-slate-400">{{ j.period }}</ink-text>
                  </ink-view>
                  <ink-text class="mb-1.5 text-sky-700">{{ j.company }}</ink-text>
                  @for (p of j.points; track p) {
                    <ink-view class="mb-0.5 flex-row gap-2">
                      <ink-text class="text-sky-500">•</ink-text>
                      <ink-text class="flex-1 text-slate-600">{{ p }}</ink-text>
                    </ink-view>
                  }
                </ink-view>
              }
            </app-section>
            <app-section title="Education">
              @for (e of education; track e.degree) {
                <ink-view class="flex-row justify-between">
                  <ink-view>
                    <ink-text class="font-bold text-slate-900">{{ e.degree }}</ink-text>
                    <ink-text class="text-slate-500">{{ e.school }}</ink-text>
                  </ink-view>
                  <ink-text class="text-slate-400">{{ e.year }}</ink-text>
                </ink-view>
              }
            </app-section>
          </ink-view>
        </ink-view>
      </ink-page>
    </ink-document>
  `,
})
export class Resume {
  @Input() name = '';
  @Input() title = '';
  @Input() contact: string[] = [];
  @Input() summary = '';
  @Input() skills: { group: string; items: string[] }[] = [];
  @Input() jobs: { role: string; company: string; period: string; points: string[] }[] = [];
  @Input() education: { degree: string; school: string; year: string }[] = [];
  initials = () =>
    this.name
      .split(' ')
      .map((n) => n[0])
      .join('');
}

@Component({
  selector: 'app-features',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="title">
      <ink-page size="A5" [style]="{ padding: 30 }">
        <ink-footer>
          <ink-text [render]="footer" />
          <ng-template #footer let-page>Page {{ page.pageNumber }} of {{ page.totalPages }}</ng-template>
        </ink-footer>
        <ink-text [inkStyle]="{ fontSize: 12, '@media (min-width: 400pt)': { fontSize: 18 } }">{{
          title
        }}</ink-text>
        <ink-view class="flex-row gap-2">
          @for (item of items; track item; let i = $index) {
            <ink-text [class]="i === 0 ? 'font-bold' : 'text-gray-500'">{{ item }}</ink-text>
          }
        </ink-view>
        <ink-text
          >A <ink-link href="https://example.com">link</ink-link> and a note
          <ink-note>Read me</ink-note>.</ink-text
        >
        <ink-image src="logo" [style]="{ width: 24, height: 24 }" />
        <ink-svg [width]="60" [height]="30" viewBox="0 0 60 30">
          <ink-circle [cx]="15" [cy]="15" [r]="10" fill="#0ea5e9" />
          <ink-text [x]="30" [y]="20" [fontSize]="10">S<ink-tspan fill="red">vg</ink-tspan></ink-text>
        </ink-svg>
        <ink-canvas [width]="40" [height]="20" [paint]="paint" />
        <ink-view class="flex-row gap-2">
          <ink-text-field name="name" value="Ada" class="h-5 w-24 border border-gray-300" />
          <ink-checkbox name="agree" checked class="h-3 w-3 border border-gray-400" />
        </ink-view>
      </ink-page>
    </ink-document>
  `,
})
export class Features {
  @Input() title = '';
  @Input() items: string[] = [];
  readonly paint = (
    p: { rect(x: number, y: number, w: number, h: number): { fill(c: string): void } },
    w: number,
    h: number,
  ) => {
    p.rect(0, 0, w, h).fill('#fde68a');
  };
}

@Component({
  selector: 'app-misplaced',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: '<ink-document><ink-page><ink-text>Hi <ink-view /></ink-text></ink-page></ink-document>',
})
export class Misplaced {}
