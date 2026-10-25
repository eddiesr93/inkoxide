/**
 * Renders every example to out/<name>.pdf and its first page, framed as a sheet of paper, to
 * packages/react/media/<name>.png (needs poppler's pdftoppm). Then rewrites the generated parts
 * of packages/react/README.md from the example sources, so the README shows exactly the code
 * that produced each picture:
 *
 * - `<!-- quickstart:start -->`: receipt.tsx, the quick start;
 * - `<!-- gallery:start -->`: the thumbnails;
 * - `<!-- examples:start -->`: each example's description and source;
 *
 * and the table of examples in examples/README.md (`<!-- examples:start -->`).
 *
 *   pnpm --filter @inkoxide/examples render [name…]
 *
 * With `--readme` it only rewrites the READMEs, without the engine or poppler, as
 * scripts/check-generated.sh does to find a README the examples have left behind.
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { type ComponentType, createElement } from 'react';

import { decodePng, encodePng, type Rgb } from './png.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, 'out');
const MEDIA = join(HERE, '..', 'packages', 'react', 'media');
const README = join(HERE, '..', 'packages', 'react', 'README.md');
const EXAMPLES_README = join(HERE, 'README.md');

/** The gallery, in README order: what each example shows, and what its first page holds. */
const EXAMPLES = [
  {
    name: 'invoice',
    title: 'Invoice',
    shows:
      'An SVG logo, a table whose header repeats on every page, totals kept together and page numbers in the footer.',
    alt: 'Invoice PDF: logo, sender and client addresses, a four-row line-item table, subtotal, tax and a highlighted total due.',
  },
  {
    name: 'report',
    title: 'Multi-page report',
    shows:
      'Three KPI cards, a bar chart drawn with SVG, a 60-row table across three pages, a header from page 2 on, and bookmarks.',
    alt: 'First page of a sales report PDF: three KPI cards, a monthly revenue bar chart and the start of a regional sales table.',
  },
  {
    name: 'certificate',
    title: 'Certificate',
    shows:
      'A landscape page, nested borders, the Inter Display face, a drawn seal and an absolutely positioned serial number.',
    alt: 'Landscape certificate PDF: a double border, "Certificate of Completion", the recipient name, two signature lines and a gold seal.',
  },
  {
    name: 'letter',
    title: 'Business letter',
    shows:
      'US Letter, justified and hyphenated paragraphs, a mailto: link and a footer between the page margins.',
    alt: 'Business letter PDF: a letterhead, the date and address block, four justified paragraphs and a signature.',
  },
  {
    name: 'resume',
    title: 'Résumé',
    shows: 'A CSS grid with a full-height sidebar, tags that wrap, and jobs that never split across pages.',
    alt: 'Résumé PDF: a dark sidebar with initials, contact details and skill tags, and a main column with profile, experience and education.',
  },
  {
    name: 'form',
    title: 'Fillable form',
    shows: 'Text fields, a select, radio buttons and checkboxes that can be filled in any PDF viewer.',
    alt: 'Membership application PDF with text fields, a plan select, two radio buttons and two consent checkboxes.',
  },
];
const QUICKSTART = { name: 'receipt', shows: "The README's quick start: an A6 receipt with a total." };
const ALL = [QUICKSTART.name, ...EXAMPLES.map((e) => e.name)];

/** Page width in the screenshot, in pixels; the frame adds a margin around it. */
const WIDTH = 900;
const MARGIN = 36;
const CANVAS = [243, 244, 246] as const; // gray-100

/** The page on a light grey canvas with a soft shadow and a hairline edge, like a sheet of paper. */
function frame(page: Rgb): Rgb {
  const width = page.width + 2 * MARGIN;
  const height = page.height + 2 * MARGIN;
  const data = new Uint8Array(width * height * 3);
  const blur = 18;
  const dy = 6;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      // Distance from the shadow's rectangle (the page, moved down by dy).
      const dx0 = Math.max(MARGIN - x, 0, x - (MARGIN + page.width - 1));
      const dy0 = Math.max(MARGIN + dy - y, 0, y - (MARGIN + dy + page.height - 1));
      const t = Math.min(Math.hypot(dx0, dy0) / blur, 1);
      const shade = 0.16 * (1 - t) ** 2;
      const i = (y * width + x) * 3;
      for (let c = 0; c < 3; c++) data[i + c] = Math.round((CANVAS[c] as number) * (1 - shade));
    }
  }
  for (let y = 0; y < page.height; y++) {
    for (let x = 0; x < page.width; x++) {
      const edge = x === 0 || y === 0 || x === page.width - 1 || y === page.height - 1;
      const src = (y * page.width + x) * 3;
      const dst = ((y + MARGIN) * width + x + MARGIN) * 3;
      for (let c = 0; c < 3; c++) data[dst + c] = edge ? 222 : (page.data[src + c] as number);
    }
  }
  return { width, height, data };
}

mkdirSync(OUT, { recursive: true });
mkdirSync(MEDIA, { recursive: true });
const readmeOnly = process.argv[2] === '--readme';
const names = readmeOnly ? [] : process.argv.length > 2 ? process.argv.slice(2) : ALL;

for (const name of names) {
  // Imported here, so that `--readme` runs without the engine.
  const { renderToBuffer } = await import('@inkoxide/react');
  const mod = (await import(`./${name}.tsx`)) as { default: ComponentType<object>; sample: object };
  const t0 = performance.now();
  const pdf = await renderToBuffer(createElement(mod.default, mod.sample));
  const ms = performance.now() - t0;
  const file = join(OUT, `${name}.pdf`);
  writeFileSync(file, pdf);
  const raw = join(OUT, `${name}-page1`);
  const width = String(name === QUICKSTART.name ? WIDTH / 2 : WIDTH);
  execFileSync('pdftoppm', [
    '-png',
    '-singlefile',
    '-f',
    '1',
    '-l',
    '1',
    '-scale-to-x',
    width,
    '-scale-to-y',
    '-1',
    file,
    raw,
  ]);
  const png = encodePng(frame(decodePng(readFileSync(`${raw}.png`))));
  writeFileSync(join(MEDIA, `${name}.png`), png);
  rmSync(`${raw}.png`);
  console.log(`${name}: ${(pdf.byteLength / 1024).toFixed(0)} KB, ${ms.toFixed(1)} ms -> media/${name}.png`);
}

/**
 * The example's source as it runs when copied into a new project with no setup (CommonJS, no
 * tsconfig): the JSX runtime named in a pragma, and a plain render call at the end, without the
 * guard that keeps render.ts's imports from rendering (nor the import it needs) and without a
 * top-level await. The guard compares real paths, so that running an example through a symlink
 * renders it too.
 */
function source(name: string): string {
  const code = readFileSync(join(HERE, `${name}.tsx`), 'utf8');
  const using = /^import \{ realpathSync \} from 'node:fs';\n\n/m;
  const guard =
    /^if \(process\.argv\[1\] && realpathSync\(process\.argv\[1\]\) === import\.meta\.filename\)\s+await (renderToFile\(.*\);)$/m;
  if (!using.test(code)) throw new Error(`${name}.tsx does not import realpathSync on its own line`);
  if (!guard.test(code)) throw new Error(`${name}.tsx does not end with the guarded renderToFile call`);
  return `/** @jsxRuntime automatic */\n${code.replace(using, '').replace(guard, '$1').trimEnd()}`;
}

const fence = (code: string) => `\`\`\`tsx\n${code}\n\`\`\``;

function gallery(): string {
  const cell = (e: (typeof EXAMPLES)[number]) =>
    `<td align="center" valign="top" width="33%"><a href="#example-${e.name}"><img src="media/${e.name}.png" alt="${e.alt}" width="260"></a><br><sub><b>${e.title}</b></sub></td>`;
  const rows: string[] = [];
  for (let i = 0; i < EXAMPLES.length; i += 3) {
    rows.push(
      `<tr>\n${EXAMPLES.slice(i, i + 3)
        .map(cell)
        .join('\n')}\n</tr>`,
    );
  }
  return `<table>\n${rows.join('\n')}\n</table>`;
}

function examples(): string {
  return EXAMPLES.map(
    (e) => `<a id="example-${e.name}"></a>

### ${e.title}

${e.shows}

<details>
<summary>Source: <code>${e.name}.tsx</code></summary>

${fence(source(e.name))}

</details>`,
  ).join('\n\n');
}

/** The table of examples/README.md. */
function table(): string {
  const rows = [QUICKSTART, ...EXAMPLES].map((e) => `| \`${e.name}.tsx\` | ${e.shows} |`);
  return ['| File | Shows |', '|---|---|', ...rows].join('\n');
}

function replaceBetween(file: string, text: string, key: string, body: string): string {
  const start = `<!-- ${key}:start (generated by examples/render.ts) -->`;
  const end = `<!-- ${key}:end -->`;
  const a = text.indexOf(start);
  const b = text.indexOf(end);
  if (a < 0 || b < a) throw new Error(`${file} has no ${start} … ${end} markers`);
  return `${text.slice(0, a + start.length)}\n\n${body}\n\n${text.slice(b)}`;
}

let readme = readFileSync(README, 'utf8');
readme = replaceBetween(README, readme, 'quickstart', fence(source(QUICKSTART.name)));
readme = replaceBetween(README, readme, 'gallery', gallery());
readme = replaceBetween(README, readme, 'examples', examples());
writeFileSync(README, readme);
console.log('README: quick start, gallery and examples updated');
writeFileSync(
  EXAMPLES_README,
  replaceBetween(EXAMPLES_README, readFileSync(EXAMPLES_README, 'utf8'), 'examples', table()),
);
console.log('examples/README.md: table updated');
