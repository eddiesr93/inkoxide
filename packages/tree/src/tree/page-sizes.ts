import type { PagePreset } from '../components/index.ts';

type Size = { width: number; height: number };

/**
 * Page presets in points, portrait: the ISO 216 A and B series, the ISO 269 C series, RA and
 * SRA (ISO 217), the US sizes, Folio and ID-1 (ISO/IEC 7810). Millimetre sizes are rounded to
 * 0.01 pt.
 */
export const PAGE_SIZES: Record<string, Size> = {
  '4A0': { width: 4767.87, height: 6740.79 },
  '2A0': { width: 3370.39, height: 4767.87 },
  A0: { width: 2383.94, height: 3370.39 },
  A1: { width: 1683.78, height: 2383.94 },
  A2: { width: 1190.55, height: 1683.78 },
  A3: { width: 841.89, height: 1190.55 },
  A4: { width: 595.28, height: 841.89 },
  A5: { width: 419.53, height: 595.28 },
  A6: { width: 297.64, height: 419.53 },
  A7: { width: 209.76, height: 297.64 },
  A8: { width: 147.4, height: 209.76 },
  A9: { width: 104.88, height: 147.4 },
  A10: { width: 73.7, height: 104.88 },
  B0: { width: 2834.65, height: 4008.19 },
  B1: { width: 2004.09, height: 2834.65 },
  B2: { width: 1417.32, height: 2004.09 },
  B3: { width: 1000.63, height: 1417.32 },
  B4: { width: 708.66, height: 1000.63 },
  B5: { width: 498.9, height: 708.66 },
  B6: { width: 354.33, height: 498.9 },
  B7: { width: 249.45, height: 354.33 },
  B8: { width: 175.75, height: 249.45 },
  B9: { width: 124.72, height: 175.75 },
  B10: { width: 87.87, height: 124.72 },
  C0: { width: 2599.37, height: 3676.54 },
  C1: { width: 1836.85, height: 2599.37 },
  C2: { width: 1298.27, height: 1836.85 },
  C3: { width: 918.43, height: 1298.27 },
  C4: { width: 649.13, height: 918.43 },
  C5: { width: 459.21, height: 649.13 },
  C6: { width: 323.15, height: 459.21 },
  C7: { width: 229.61, height: 323.15 },
  C8: { width: 161.57, height: 229.61 },
  C9: { width: 113.39, height: 161.57 },
  C10: { width: 79.37, height: 113.39 },
  RA0: { width: 2437.8, height: 3458.27 },
  RA1: { width: 1729.13, height: 2437.8 },
  RA2: { width: 1218.9, height: 1729.13 },
  RA3: { width: 864.57, height: 1218.9 },
  RA4: { width: 609.45, height: 864.57 },
  SRA0: { width: 2551.18, height: 3628.35 },
  SRA1: { width: 1814.17, height: 2551.18 },
  SRA2: { width: 1275.59, height: 1814.17 },
  SRA3: { width: 907.09, height: 1275.59 },
  SRA4: { width: 637.8, height: 907.09 },
  Letter: { width: 612, height: 792 },
  Legal: { width: 612, height: 1008 },
  Tabloid: { width: 792, height: 1224 },
  Executive: { width: 522, height: 756 },
  Folio: { width: 612, height: 936 },
  ID1: { width: 153.01, height: 242.65 },
} satisfies Record<PagePreset, Size>;

/** The presets, for error messages, with each series as a range: `4A0, 2A0, A0–A10, …, ID1`. */
export const PRESET_NAMES = presetNames(Object.keys(PAGE_SIZES));

function presetNames(names: readonly string[]): string {
  const groups: Array<{ series: string; names: string[] }> = [];
  for (const name of names) {
    // `A` for `A4`; a name outside a series is a group of its own.
    const series = /^([A-Z]+)\d+$/.exec(name)?.[1] ?? name;
    const last = groups.at(-1);
    if (last?.series === series) last.names.push(name);
    else groups.push({ series, names: [name] });
  }
  return groups
    .map(({ names: [first, ...rest] }) => (rest.length > 0 ? `${first}–${rest.at(-1)}` : first))
    .join(', ');
}

/** `PAGE_SIZES` by lower-case name: preset names match in any case. */
const PRESETS = new Map(Object.entries(PAGE_SIZES).map(([name, size]) => [name.toLowerCase(), size]));

/**
 * The size of a preset name, in any case, optionally with `-landscape` (`'a4'`,
 * `'Letter-landscape'`); undefined for any other string.
 */
export function presetSize(name: string): Size | undefined {
  const [base = '', orient, ...rest] = name.toLowerCase().split('-');
  const preset = PRESETS.get(base);
  if (!preset || rest.length > 0 || (orient !== undefined && orient !== 'landscape')) return undefined;
  return orient ? { width: preset.height, height: preset.width } : { ...preset };
}
