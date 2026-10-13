/**
 * CSS font matching: `FontRegistry.resolve` and the engine's `FontSet::select`
 * (crates/core/src/fonts/mod.rs) pass the same table of cases.
 */
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { type FontFace, FontRegistry } from '../src/index.ts';

interface Case {
  name: string;
  faces: string;
  families: string[];
  weight: number;
  style: 'normal' | 'italic';
  id: string;
  exact: boolean;
}

const table = JSON.parse(
  readFileSync(new URL('../../../fixtures/fonts/matching.json', import.meta.url), 'utf8'),
) as { faces: Record<string, FontFace[]>; cases: Case[] };

describe('font matching', () => {
  it('has the shared cases', () => {
    expect(table.cases.length).toBeGreaterThan(20);
  });

  it.each(table.cases.map((c) => [c.name, c] as const))('%s', (_, c) => {
    const registry = new FontRegistry();
    for (const face of table.faces[c.faces] ?? []) registry.register(face);
    const r = registry.resolve(c.families, c.weight, c.style);
    expect({ id: r.id, exact: r.exact }).toEqual({ id: c.id, exact: c.exact });
  });
});
