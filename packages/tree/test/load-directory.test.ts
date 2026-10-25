/**
 * `FontRegistry.loadDirectory` (deprecated) reads font files in Node, without importing `node:fs`,
 * so that the package loads where there is no file system; there it throws.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { FontRegistry } from '../src/index.ts';

const FONTS = fileURLToPath(new URL('../../../fixtures/fonts/', import.meta.url));

afterEach(() => {
  vi.restoreAllMocks();
});

describe('FontRegistry.loadDirectory', () => {
  it('registers the faces whose file it reads, and lists the others', () => {
    const r = new FontRegistry();
    const result = r.loadDirectory(FONTS, [
      { id: 'Inter-Regular', family: 'Inter', file: 'Inter-Regular.ttf' },
      { id: 'Missing', family: 'Missing', file: 'Missing.ttf' },
      { id: 'No-File', family: 'No File' },
    ]);
    expect(result).toEqual({ loaded: ['Inter-Regular'], missing: ['Missing'] });
    const bytes = r.bytes('Inter-Regular');
    expect(bytes && Buffer.from(bytes).equals(readFileSync(`${FONTS}Inter-Regular.ttf`))).toBe(true);
  });

  it('throws where there is no node:fs', () => {
    vi.spyOn(process, 'getBuiltinModule').mockReturnValue(undefined);
    const r = new FontRegistry();
    expect(() =>
      r.loadDirectory(FONTS, [{ id: 'Inter-Regular', family: 'Inter', file: 'Inter-Regular.ttf' }]),
    ).toThrow('fonts.loadDirectory reads files, which only Node can do here.');
    expect(r.ids()).toEqual([]);
  });
});
