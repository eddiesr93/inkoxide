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

describe('variable faces', () => {
  const registry = () =>
    new FontRegistry()
      .register({ id: 'Var', family: 'Var', weightRange: [200, 700] })
      .register({
        id: 'Var-italic',
        family: 'Var',
        style: 'italic',
        weightRange: [200, 700],
        variations: { ital: 1 },
      })
      .register({ id: 'Static-900', family: 'Var', weight: 900 });

  it('draw a weight in their range at that weight, and one outside at the nearer end', () => {
    const r = registry();
    expect(r.resolve(['Var'], 450, 'normal')).toEqual({ id: 'Var@wght=450', exact: true });
    expect(r.resolve(['Var'], 100, 'normal')).toEqual({
      id: 'Var@wght=200',
      exact: false,
      wanted: 'Var 100',
    });
    // Above 500: heavier first, so the static 900 wins over the range's end.
    expect(r.resolve(['Var'], 800, 'normal')).toEqual({ id: 'Static-900', exact: false, wanted: 'Var 800' });
    expect(r.resolve(['Var'], 300, 'italic')).toEqual({ id: 'Var-italic@ital=1,wght=300', exact: true });
    expect(r.decl('Var@wght=450')).toEqual({
      id: 'Var@wght=450',
      family: 'Var',
      weight: 450,
      source: 'Var',
      variations: { wght: 450 },
    });
    expect(r.decl('Var-italic@ital=1,wght=300')).toEqual({
      id: 'Var-italic@ital=1,wght=300',
      family: 'Var',
      weight: 300,
      style: 'italic',
      source: 'Var-italic',
      variations: { ital: 1, wght: 300 },
    });
    expect(r.instancesOf('Var')).toEqual(['Var@wght=450', 'Var@wght=200']);
  });

  it('take fontVariationSettings, whose wght wins over the weight', () => {
    const r = registry();
    expect(r.resolve(['Var'], 400, 'normal', { wdth: 75, wght: 650 }).id).toBe('Var@wdth=75,wght=650');
    expect(r.decl('Var@wdth=75,wght=650')).toMatchObject({
      weight: 650,
      variations: { wdth: 75, wght: 650 },
    });
    // A static face takes them too: the engine applies those its font has.
    expect(r.resolve(['Var'], 900, 'normal', { wdth: 75 }).id).toBe('Static-900@wdth=75');
    expect(r.resolve(['Var'], 900, 'normal').id).toBe('Static-900');
  });

  it('keep the instances used last, however many a long-running process meets', () => {
    const r = registry();
    const first = r.resolve(['Var'], 400, 'normal', { wdth: 0 }).id;
    for (let i = 1; i <= 5000; i++) r.resolve(['Var'], 400, 'normal', { wdth: i });
    const last = r.resolve(['Var'], 400, 'normal', { wdth: 5000 }).id;
    expect(r.has(last)).toBe(true);
    expect(r.decl(last)).toMatchObject({ source: 'Var', variations: { wdth: 5000 } });
    // The first made room, and comes back when asked for again.
    expect(r.has(first)).toBe(false);
    expect(r.resolve(['Var'], 400, 'normal', { wdth: 0 }).id).toBe(first);
    expect(r.has(first)).toBe(true);
  });
});
