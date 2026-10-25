import { describe, expect, it } from 'vitest';

import { BUNDLED_ASSETS } from '../src/browser-host.ts';
import { DEFAULT_FACES } from '../src/fonts.ts';
import { HYPHENATION_FILES } from '../src/hyphenation.ts';

describe('the bundled assets of the browser host', () => {
  it('are the files of fonts.ts and hyphenation.ts', () => {
    const files = [
      ...DEFAULT_FACES.map((f) => `fonts/${f.file}`),
      ...Object.values(HYPHENATION_FILES).map((f) => `hyphenation/${f}`),
    ];
    expect([...BUNDLED_ASSETS].sort()).toEqual(files.sort());
  });
});
