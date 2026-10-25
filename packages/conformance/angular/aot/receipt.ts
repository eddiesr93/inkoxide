// The receipt of examples/receipt.tsx as an application's build compiles it, ahead of time, with
// signal inputs (which only Angular's compiler reads): test/angular-aot.test.ts compiles it with
// `ngc` and renders it, with no JIT compiler loaded.
import { Component, computed, CUSTOM_ELEMENTS_SCHEMA, input } from '@angular/core';

@Component({
  selector: 'app-receipt',
  schemas: [CUSTOM_ELEMENTS_SCHEMA],
  template: `
    <ink-document [title]="'Receipt for ' + customer()">
      <ink-page size="A6" class="p-8 text-xs text-gray-900">
        <ink-text class="text-lg font-bold">Receipt</ink-text>
        <ink-text class="mb-4 text-gray-500">for {{ customer() }}</ink-text>
        @for (l of lines(); track l.name) {
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
  readonly customer = input.required<string>();
  readonly lines = input.required<{ name: string; amount: number }[]>();
  readonly total = computed(() => this.lines().reduce((sum, l) => sum + l.amount, 0));
}
