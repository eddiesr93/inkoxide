/**
 * The build plugin: a browser bundle gets the precompiled entry with the table of the classes in
 * its sources, and none of Tailwind's compiler; a server bundle keeps the compiler; a dev server
 * reloads the page when a file brings a new class.
 */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliCompressSync } from 'node:zlib';

import * as esbuild from 'esbuild';
import { build as viteBuild, type Rollup } from 'vite';
import { afterAll, describe, expect, it } from 'vitest';

import { inkoxideTailwind, inkoxideTailwindEsbuild } from '../src/plugin.ts';

const SRC = fileURLToPath(new URL('../src/', import.meta.url));

/** A project whose page maps a class string, written in its source and safelisted. */
const dir = mkdtempSync(join(tmpdir(), 'inkoxide-plugin-'));
mkdirSync(join(dir, 'src'));
writeFileSync(
  join(dir, 'src/main.ts'),
  `import { initTailwind, tw } from '@inkoxide/tailwind';
await initTailwind();
const extra = ['bg', 'lime', '900'].join('-');
export const styles = [tw('p-4 text-red-500'), tw(extra)];
`,
);

afterAll(() => rmSync(dir, { recursive: true, force: true }));

/** The module `code` exports, run in this process. */
async function run(code: string): Promise<{ styles: unknown[] }> {
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

const chunks = (out: unknown) =>
  (Array.isArray(out) ? out : [out]).flatMap((o) => (o as Rollup.RolldownOutput).output);

describe('the build plugin', () => {
  it('bundles the table for the browser, and not the compiler', async () => {
    const out = await viteBuild({
      root: dir,
      logLevel: 'silent',
      configFile: false,
      plugins: [inkoxideTailwind({ safelist: ['bg-lime-900'] })],
      build: {
        write: false,
        target: 'es2023',
        rolldownOptions: {
          input: join(dir, 'src/main.ts'),
          preserveEntrySignatures: 'exports-only',
          output: { codeSplitting: false },
        },
      },
    });
    const code = chunks(out)
      .map((c) => ('code' in c ? c.code : ''))
      .join('\n');
    expect(code).not.toContain('candidatesToCss');
    expect(code).toContain('text-red-500');
    const { styles } = await run(code);
    expect(styles).toEqual([
      {
        paddingTop: 12,
        paddingRight: 12,
        paddingBottom: 12,
        paddingLeft: 12,
        color: expect.any(Array),
      },
      { backgroundColor: expect.any(Array) },
    ]);
  });

  it('leaves server bundles to the compiler', () => {
    const plugin = inkoxideTailwind();
    const precompiled = plugin.resolveId.call({}, '@inkoxide/tailwind');
    expect(precompiled).toMatch(/precompiled\.ts$/);
    expect(plugin.resolveId.call({}, '@inkoxide/tailwind', undefined, { ssr: true })).toBeNull();
    const server = { environment: { config: { consumer: 'server' } } };
    expect(plugin.resolveId.call(server, '@inkoxide/tailwind')).toBeNull();
    expect(plugin.resolveId.call({}, '@inkoxide/style')).toBeNull();
  });

  it('works with esbuild, for the browser only', async () => {
    const result = await esbuild.build({
      entryPoints: [join(dir, 'src/main.ts')],
      absWorkingDir: dir,
      bundle: true,
      write: false,
      format: 'esm',
      platform: 'browser',
      target: 'es2023',
      metafile: true,
      logLevel: 'silent',
      plugins: [inkoxideTailwindEsbuild({ safelist: ['bg-lime-900'] })],
    });
    const inputs = Object.keys(result.metafile.inputs);
    expect(inputs.some((i) => i.endsWith('table.ts'))).toBe(true);
    const { styles } = await run(result.outputFiles[0]?.text ?? '');
    expect(styles[1]).toEqual({ backgroundColor: expect.any(Array) });
  });

  it('keeps the compiler out of the browser entry, which is small', async () => {
    const result = await esbuild.build({
      entryPoints: [join(SRC, 'precompiled.ts')],
      // @inkoxide/react loads it anyway.
      external: ['@inkoxide/style', '@inkoxide/style/*'],
      bundle: true,
      write: false,
      format: 'esm',
      platform: 'browser',
      target: 'es2023',
      minify: true,
      metafile: true,
      logLevel: 'silent',
    });
    const inputs = Object.keys(result.metafile.inputs);
    for (const heavy of ['tailwindcss', '@tailwindcss', 'postcss', 'compile.ts', 'supported.ts']) {
      expect(
        inputs.filter((i) => i.includes(heavy)),
        heavy,
      ).toEqual([]);
    }
    const bytes = brotliCompressSync(result.outputFiles[0]?.contents ?? new Uint8Array()).length;
    expect(bytes).toBeLessThan(12_000);
  });

  it('reloads the page in a dev server when a file brings a new class', async () => {
    writeFileSync(join(dir, 'classes.json'), '["p-4"]');
    const plugin = inkoxideTailwind({ sources: ['src', 'classes.json'] });
    plugin.configResolved({ root: dir });
    await plugin.buildStart.call({});
    const sent: unknown[] = [];
    const invalidated: unknown[] = [];
    const environment = (consumer: string) => ({
      config: { consumer },
      moduleGraph: {
        getModuleById: (id: string) => ({ id }),
        invalidateModule: (mod: never) => void invalidated.push(mod),
      },
      hot: { send: (payload: unknown) => void sent.push(payload) },
    });
    const client = { environment: environment('client') };
    type Update = 'create' | 'update' | 'delete';
    const hot = (file: string, text: string, type: Update = 'update', env = client) =>
      plugin.hotUpdate.call(env, { type, file, read: () => text });
    const b = join(dir, 'src/b.tsx');
    // A class it has, a file it does not read, one outside its sources, a file removed and the
    // server's environment: nothing.
    expect(await hot(b, 'p-4')).toBeUndefined();
    expect(await hot(join(tmpdir(), 'elsewhere.tsx'), 'm-3')).toBeUndefined();
    expect(await hot(join(dir, 'src/x.css'), 'm-3')).toBeUndefined();
    expect(await hot(b, 'm-3', 'delete')).toBeUndefined();
    expect(await hot(b, 'm-3', 'update', { environment: environment('server') })).toBeUndefined();
    expect(sent).toEqual([]);
    // A file created with a new class, and a source named whatever its type.
    expect(await hot(b, '<View className="m-3" />', 'create')).toEqual([]);
    expect(await hot(join(dir, 'classes.json'), '["m-5"]')).toEqual([]);
    expect(sent).toEqual([{ type: 'full-reload' }, { type: 'full-reload' }]);
    expect(invalidated).toHaveLength(2);
    const table = await plugin.load.call({}, join(SRC, 'table.ts'));
    expect(table?.code).toContain('m-3');
    expect(table?.code).toContain('m-5');
  });
});
