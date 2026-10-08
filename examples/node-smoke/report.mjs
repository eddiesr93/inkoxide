// A ~100-page report (paragraphs + 40 tables, 2 fonts, footer with page
// numbers), the same shape as scripts/fixtures/report.py. Seeded, so
// every run builds the same document.

const INK = [0.1, 0.11, 0.14, 1];
const MUTED = [0.4, 0.42, 0.48, 1];
const LINE = [0.85, 0.86, 0.9, 1];
const WORDS = (
  'analysis revenue quarterly performance growth market segment customer retention ' +
  'operational efficiency forecast budget allocation strategic initiative regional ' +
  'distribution supply chain logistics procurement compliance governance framework ' +
  'implementation measurable outcomes stakeholder engagement sustainability digital'
).split(' ');

// mulberry32
function rng(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function report(sections = 40) {
  const r = rng(42);
  const int = (lo, hi) => lo + Math.floor(r() * (hi - lo + 1));
  const word = () => WORDS[int(0, WORDS.length - 1)];
  const words = (lo, hi) => Array.from({ length: int(lo, hi) }, word).join(' ');
  const sentence = () => {
    const s = words(8, 18);
    return s[0].toUpperCase() + s.slice(1) + '.';
  };
  const run = (text, font = 'Inter-Regular', size = 10, color = INK) => ({ text, font, size, color });
  const para = (n) => ({
    type: 'text',
    style: { textAlign: 'justify', lineHeight: 1.45, hyphenate: true, margin: { bottom: 8 } },
    runs: [run(Array.from({ length: n ?? int(3, 7) }, sentence).join(' '))],
  });
  const cell = (t, align = 'left', font = 'Inter-Regular') => ({
    style: {
      padding: { top: 3, right: 5, bottom: 3, left: 5 },
      borderWidth: { bottom: 0.5 },
      borderColor: { bottom: LINE },
    },
    children: [{ type: 'text', style: { textAlign: align }, runs: [run(t, font, 8.5)] }],
  });
  const table = (k) => {
    const head = ['Code', 'Description', 'Units', 'Price', 'Amount'];
    const rows = [
      {
        backgroundColor: [0.93, 0.95, 0.99, 1],
        cells: head.map((h, i) => cell(h, i < 2 ? 'left' : 'right', 'Inter-Bold')),
      },
    ];
    for (let i = int(20, 40); i > 0; i--) {
      const units = int(1, 500);
      const price = 1 + r() * 899;
      rows.push({
        cells: [
          cell(`S${k}-${i}`),
          cell(words(2, 6)),
          cell(String(units), 'right'),
          cell(price.toFixed(2), 'right'),
          cell((units * price).toFixed(2), 'right'),
        ],
      });
    }
    return {
      type: 'table',
      headerRows: 1,
      style: { margin: { top: 4, bottom: 12 } },
      columns: [{ width: 60 }, { width: '1fr' }, { width: 50 }, { width: 70 }, { width: 80 }],
      rows,
    };
  };

  const children = [];
  for (let s = 1; s <= sections; s++) {
    children.push({
      type: 'text',
      break: { minPresenceAhead: 60 },
      style: { margin: { top: 10, bottom: 6 } },
      runs: [run(`${s}. Section ${s}: ${words(3, 3)}`, 'Inter-Bold', 15)],
    });
    for (let p = int(7, 12); p > 0; p--) children.push(para());
    children.push(table(s), para(2));
  }
  const small = (text) => run(text, 'Inter-Regular', 8, MUTED);
  const footer = {
    type: 'box',
    style: {
      flexDirection: 'row',
      justifyContent: 'space-between',
      padding: { top: 12, right: 48, bottom: 20, left: 48 },
    },
    children: [
      { type: 'text', runs: [small('Quarterly report')] },
      {
        type: 'text',
        runs: [
          small('Page '),
          { placeholder: 'pageNumber', font: 'Inter-Regular', size: 8, color: MUTED },
          small(' of '),
          { placeholder: 'totalPages', font: 'Inter-Regular', size: 8, color: MUTED },
        ],
      },
    ],
  };
  return {
    version: 1,
    meta: { title: 'Report', lang: 'en' },
    fonts: [
      { id: 'Inter-Regular', family: 'Inter' },
      { id: 'Inter-Bold', family: 'Inter', weight: 700 },
    ],
    pages: [
      {
        size: { width: 595.28, height: 841.89 },
        margin: { top: 48, right: 48, bottom: 56, left: 48 },
        footer,
        children,
      },
    ],
  };
}
