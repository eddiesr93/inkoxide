import { describe, expect, it } from 'vitest';

import { BUNDLED_ASSETS, configureBrowser } from '../src/browser-host.ts';
import { DEFAULT_FACES } from '../src/fonts.ts';
import { engineHost } from '../src/host.ts';
import { HYPHENATION_FILES } from '../src/hyphenation.ts';
import type { Request, Response } from '../src/worker.ts';

describe('the bundled assets of the browser host', () => {
  it('are the files of fonts.ts and hyphenation.ts', () => {
    const files = [
      ...DEFAULT_FACES.map((f) => `fonts/${f.file}`),
      ...Object.values(HYPHENATION_FILES).map((f) => `hyphenation/${f}`),
    ];
    expect([...BUNDLED_ASSETS].sort()).toEqual(files.sort());
  });
});

describe('a worker the application makes', () => {
  it('is the one the engine starts in, in place of the one of the package', async () => {
    // Answers as the engine's worker does, without an engine.
    const asked: string[] = [];
    const made: object[] = [];
    const make = (): Worker => {
      const worker = {
        onmessage: null as ((e: { data: Response }) => void) | null,
        onerror: null,
        postMessage(message: Request) {
          asked.push(message.op);
          const value = message.op === 'version' ? { engine: '0', schema: 1, gitSha: '' } : 1;
          queueMicrotask(() => worker.onmessage?.({ data: { id: message.id, value } }));
        },
        terminate() {},
      };
      made.push(worker);
      return worker as unknown as Worker;
    };
    configureBrowser({ worker: make, threads: 1 });
    await engineHost().start();
    expect(made).toHaveLength(1);
    expect(asked).toEqual(['init', 'version', 'threadCount']);
    expect(engineHost().version()).toEqual({ engine: '0', schema: 1, gitSha: '' });
  });
});
