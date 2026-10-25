// The invoice of examples/invoice.tsx, which each page renders with its front end: a copy, since
// that file runs Node's code when it loads. tests/render-node.tsx checks that they are the same.
export const sample = {
  number: '2026-0142',
  issued: 'October 11, 2026',
  due: 'November 10, 2026',
  from: {
    name: 'Acme Studio',
    address: ['25 Harbor Street', 'Portland, OR 97204'],
    email: 'billing@acme.studio',
  },
  to: {
    name: 'Lincoln Elementary',
    address: ['4 Maple Avenue', 'Springfield, IL 62704'],
    email: 'office@lincoln.edu',
  },
  items: [
    { description: 'Brand identity', details: 'Logo, palette and type system', quantity: 1, price: 4800 },
    { description: 'Website design', details: 'Eight pages, desktop and mobile', quantity: 8, price: 650 },
    { description: 'Illustrations', details: 'Custom spot illustrations', quantity: 12, price: 180 },
    { description: 'Support', details: 'Hours, billed monthly', quantity: 6, price: 95 },
  ],
  taxRate: 0.08,
};
