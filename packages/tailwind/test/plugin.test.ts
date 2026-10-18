/**
 * The build plugin: a browser bundle gets the precompiled entry with the table of the classes in
 * its sources, and none of Tailwind's compiler; a server bundle keeps the compiler; a dev server
 * reloads the page when a file brings a new class.
 */
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliCompressSync } from 'node:zlib';

import { rspack, type Configuration as RspackConfiguration } from '@rspack/core';
import * as esbuild from 'esbuild';
import { build as viteBuild, type Rollup } from 'vite';
import { afterAll, describe, expect, it } from 'vitest';
import webpack, { type Configuration as WebpackConfiguration } from 'webpack';

import {
  inkoxideTailwind,
  inkoxideTailwindEsbuild,
  inkoxideTailwindWebpack,
  withInkoxideTailwind,
} from '../src/plugin.ts';
import { checkTable, TABLE_GLOBAL } from '../src/table-source.ts';

const SRC = fileURLToPath(new URL('../src/', import.meta.url));

/** A project whose page maps a class string, written in its source and safelisted. */
const dir = realpathSync(mkdtempSync(join(tmpdir(), 'inkoxide-plugin-')));
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

  /** The config of a webpack or Rspack build of the page into `out`, as an ES module. */
  const webpackConfig = (out: string, rule: object) => ({
    mode: 'production' as const,
    context: dir,
    entry: join(dir, 'src/main.ts'),
    target: 'web',
    output: { path: join(dir, out), filename: 'main.js', module: true, library: { type: 'module' as const } },
    experiments: { outputModule: true },
    optimization: { minimize: false },
    module: { rules: [{ test: /\.ts$/, ...rule }] },
    plugins: [inkoxideTailwindWebpack({ safelist: ['bg-lime-900'] })],
  });

  const built = (out: string) => readFileSync(join(dir, out, 'main.js'), 'utf8');

  it('works with webpack, for the browser only', async () => {
    const rule = { loader: fileURLToPath(new URL('./esbuild-loader.ts', import.meta.url)) };
    const build = (config: WebpackConfiguration) =>
      new Promise<void>((done, fail) =>
        webpack(config, (error, stats) => {
          if (error ?? stats?.hasErrors()) fail(error ?? new Error(stats?.toString('errors-only')));
          else done();
        }),
      );
    await build(webpackConfig('webpack', rule));
    const code = built('webpack');
    expect(code).not.toContain('candidatesToCss');
    const { styles } = await run(code);
    expect(styles[1]).toEqual({ backgroundColor: expect.any(Array) });
  });

  it('leaves webpack builds for Node to the compiler', () => {
    const tap = () => {};
    for (const target of ['node22', ['node', 'es2022'], 'async-node', 'electron-main']) {
      const options = { target, resolve: {}, module: { rules: [] } };
      const compiler = { context: dir, options, hooks: { run: { tap }, watchRun: { tap } } };
      inkoxideTailwindWebpack().apply(compiler);
      expect(options, JSON.stringify(target)).toEqual({ target, resolve: {}, module: { rules: [] } });
    }
  });

  it('works with Rspack', async () => {
    const rule = {
      loader: 'builtin:swc-loader',
      options: { jsc: { parser: { syntax: 'typescript' }, target: 'es2023' } },
    };
    await new Promise<void>((done, fail) =>
      rspack(webpackConfig('rspack', rule) as RspackConfiguration, (error, stats) => {
        if (error ?? stats?.hasErrors())
          fail(error ?? new Error(stats?.toString({ all: false, errors: true })));
        else done();
      }),
    );
    const code = built('rspack');
    expect(code).not.toContain('candidatesToCss');
    const { styles } = await run(code);
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

  it('reads in a dev server the files the build reads, and no others', async () => {
    for (const d of ['tmp', '.cache', 'node_modules/x', 'src/build'])
      mkdirSync(join(dir, d), { recursive: true });
    // Over 1 MiB: not read in a directory.
    writeFileSync(join(dir, 'src/big.tsx'), 'p-4 '.repeat(300_000));
    const plugin = inkoxideTailwind();
    plugin.configResolved({ root: dir });
    await plugin.buildStart.call({});
    const sent: unknown[] = [];
    const client = {
      environment: {
        config: { consumer: 'client' },
        moduleGraph: { getModuleById: () => undefined, invalidateModule: () => {} },
        hot: { send: (payload: unknown) => void sent.push(payload) },
      },
    };
    const hot = (file: string) =>
      plugin.hotUpdate.call(client, { type: 'update', file: join(dir, file), read: () => 'm-7' });
    // Build output, dependencies and hidden directories directly under or below the root.
    for (const file of ['tmp/a.tsx', '.cache/b.js', 'node_modules/x/c.js', 'src/big.tsx']) {
      expect(await hot(file), file).toBeUndefined();
    }
    expect(sent).toEqual([]);
    // A folder named like build output is read below the root.
    expect(await hot('src/build/d.ts')).toEqual([]);
    expect(sent).toEqual([{ type: 'full-reload' }]);
  });

  it("scans what the theme's @source directives name", async () => {
    mkdirSync(join(dir, 'theme/lib'), { recursive: true });
    writeFileSync(join(dir, 'theme/lib/card.ts'), "export const card = 'text-blue-500 p-4';");
    writeFileSync(
      join(dir, 'theme/theme.css'),
      `@import "tailwindcss" source(none);
@source "./lib";
@source inline("m-{1..2}");
@source not inline("p-4");
`,
    );
    const plugin = inkoxideTailwind({ cssFile: 'theme/theme.css' });
    plugin.configResolved({ root: dir });
    const watched: string[] = [];
    await plugin.buildStart.call({ addWatchFile: (f: string) => void watched.push(f) });
    expect(watched).toContain(join(dir, 'theme/lib/card.ts'));
    expect(watched).toContain(join(dir, 'theme/theme.css'));
    const code = (await plugin.load.call({}, join(SRC, 'table.ts')))?.code ?? '';
    for (const kept of ['text-blue-500', 'm-1', 'm-2']) expect(code).toContain(`\\"${kept}\\"`);
    // Not under the theme's sources, and never kept.
    for (const left of ['text-red-500', 'p-4']) expect(code).not.toContain(`\\"${left}\\"`);
  });

  it('wraps a Next.js config, for Turbopack and webpack', async () => {
    const cwd = process.cwd();
    process.chdir(dir);
    try {
      const userWebpack = (config: { plugins?: unknown[] }) => ({ ...config, user: true });
      const wrapped = withInkoxideTailwind(
        () => ({ reactStrictMode: true, turbopack: { resolveAlias: { a: './b.js' } }, webpack: userWebpack }),
        { safelist: ['bg-lime-900'] },
      );
      const context = { defaultConfig: {} };
      // `next start` bundles nothing: no module.
      await wrapped('phase-production-server', context);
      const module = join(dir, 'node_modules/.cache/@inkoxide/tailwind/browser.js');
      expect(existsSync(module)).toBe(false);
      const config = await wrapped('phase-production-build', context);
      expect(config.reactStrictMode).toBe(true);
      expect(config.turbopack?.resolveAlias).toEqual({
        a: './b.js',
        '@inkoxide/tailwind': { browser: './node_modules/.cache/@inkoxide/tailwind/browser.js' },
      });
      const text = readFileSync(module, 'utf8');
      expect(text).toContain('bg-lime-900');
      const pkg = JSON.parse(readFileSync(join(dirname(module), 'package.json'), 'utf8'));
      expect(pkg).toEqual({ type: 'module', sideEffects: true });
      // What Turbopack runs: the table lands where the precompiled entry looks for it.
      const global = globalThis as { [TABLE_GLOBAL]?: unknown };
      try {
        new Function(text.replace(/^export \* from .*$/m, ''))();
        expect(Object.keys(checkTable(global[TABLE_GLOBAL]).classes)).toContain('bg-lime-900');
      } finally {
        delete global[TABLE_GLOBAL];
      }
      const from = /export \* from "(.*)";/.exec(text)?.[1] ?? '';
      expect(resolve(dirname(module), from)).toBe(realpathSync(join(SRC, 'precompiled.ts')));
      const browser = config.webpack?.({}, { isServer: false, dir }) as { plugins: unknown[]; user: boolean };
      expect(browser.user).toBe(true);
      expect(browser.plugins).toHaveLength(1);
      expect(config.webpack?.({}, { isServer: true, dir })).toEqual({ user: true });
    } finally {
      process.chdir(cwd);
    }
  });
});
