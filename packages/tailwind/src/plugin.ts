/**
 * The Tailwind build plugin: in a browser bundle, `@inkoxide/tailwind` becomes `precompiled.ts`,
 * which reads a table of the classes the sources use instead of loading Tailwind's compiler (about
 * 90 KB with brotli). At the start of a build it scans the sources for classes (`scan.ts`) and
 * builds the table (`precompile.ts`); a dev server scans each file that changes, and reloads the
 * page when it has a new class. Server bundles keep the compiler.
 *
 * `inkoxideTailwind()` is a Vite plugin, and a Rollup or Rolldown one; `inkoxideTailwindEsbuild()`
 * an esbuild one. No user module is changed: templates resolve their classes at render time, as
 * with the compiler, and get the same styles.
 */
import { readFileSync, realpathSync } from 'node:fs';
import { dirname, extname, isAbsolute, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { Precompiler } from './precompile.ts';
import { candidatesIn, SCANNED, scanFiles, sourceFiles } from './scan.ts';
import type { Table } from './table-source.ts';

export interface TailwindPluginOptions {
  /** Theme CSS, as for `initTailwind({ css })`; by default the bundled theme. */
  css?: string;
  /** A file with the theme CSS, whose `@import`s resolve from its folder; in place of `css`. */
  cssFile?: string;
  /**
   * Directories and files to scan for classes (relative to the project root); by default the
   * project root, without `node_modules`, build output and hidden directories.
   */
  sources?: string[];
  /** Classes to keep whether the scan finds them or not: those put together at run time. */
  safelist?: string[];
}

/** The module the plugin puts `@inkoxide/tailwind` in place of, and the table stub it fills. */
const PRECOMPILED = realpathSync(fileURLToPath(new URL('./precompiled.ts', import.meta.url)));
const TABLE_FILE = realpathSync(fileURLToPath(new URL('./table.ts', import.meta.url)));

/** What a build scanned: the table so far and the files it read. */
interface Scan {
  precompiler: Precompiler;
  files: string[];
}

/** The theme CSS of `options` and where its imports resolve from. */
function theme(options: TailwindPluginOptions, root: string): { css?: string; base?: string } {
  if (options.cssFile === undefined) return options.css === undefined ? {} : { css: options.css };
  const file = isAbsolute(options.cssFile) ? options.cssFile : resolve(root, options.cssFile);
  return { css: readFileSync(file, 'utf8'), base: dirname(file) };
}

/** The directories and files of `options.sources` under `root`. */
const rootsOf = (options: TailwindPluginOptions, root: string): string[] =>
  (options.sources ?? ['.']).map((s) => resolve(root, s));

/** Scans the sources under `root`, without `out` (the build's output), and builds the table. */
async function scan(options: TailwindPluginOptions, root: string, out?: string): Promise<Scan> {
  const precompiler = await Precompiler.create(theme(options, root));
  const files = sourceFiles(rootsOf(options, root), new Set(out ? [resolve(root, out)] : []));
  precompiler.add(scanFiles(files));
  precompiler.add(options.safelist ?? []);
  return { precompiler, files };
}

/** Whether `file` is in one of `roots` and not in `out`. */
function inSources(file: string, roots: readonly string[], out: string | undefined): boolean {
  const under = (dir: string) => file === dir || file.startsWith(dir + sep);
  return roots.some(under) && !(out !== undefined && under(out));
}

/** A path with `/` between its parts, as bundlers write module ids. */
const slashed = (path: string): string => path.split(sep).join('/');

/** The table stub's code with `table` in it: parsed as JSON, which is faster than a literal. */
function tableModule(table: Table): string {
  return `export const TABLE = JSON.parse(${JSON.stringify(JSON.stringify(table))});\n`;
}

/** `id` without a query (`?v=…`), resolved through links, with `/` between its parts. */
function fileOf(id: string): string {
  const path = id.split('?', 1)[0] as string;
  try {
    return slashed(realpathSync(path));
  } catch {
    return slashed(path);
  }
}

/** What the plugin uses of a bundler's context: Vite's, Rollup's or Rolldown's. */
interface Context {
  addWatchFile?(file: string): void;
  environment?: { config?: { consumer?: string } };
}

/** What the plugin uses of a Vite dev server. */
interface DevServer {
  moduleGraph: {
    getModuleById(id: string): unknown;
    invalidateModule(mod: never): void;
  };
  ws: { send(payload: { type: 'full-reload' }): void };
}

/** A Vite, Rollup or Rolldown plugin. */
export interface TailwindPlugin {
  name: string;
  enforce: 'pre';
  config(): { optimizeDeps: { exclude: string[] } };
  configResolved(config: { root: string; build?: { outDir?: string } }): void;
  buildStart(this: Context): Promise<void>;
  resolveId(this: Context, id: string, importer?: string, opts?: { ssr?: boolean }): string | null;
  load(this: Context, id: string): Promise<{ code: string; map: null } | null>;
  handleHotUpdate(ctx: {
    file: string;
    server: DevServer;
    read(): string | Promise<string>;
  }): Promise<[] | undefined>;
}

/**
 * The Tailwind build plugin for Vite, Rollup and Rolldown:
 * `plugins: [inkoxideTailwind()]` in `vite.config.ts`. Server builds (Vite's SSR) keep the
 * compiler.
 */
export function inkoxideTailwind(options: TailwindPluginOptions = {}): TailwindPlugin {
  let root = process.cwd();
  let outDir: string | undefined;
  let scanned: Promise<Scan> | undefined;
  const start = () => (scanned ??= scan(options, root, outDir));
  return {
    name: 'inkoxide-tailwind',
    enforce: 'pre',
    // Vite would bundle the package ahead of time, around this plugin.
    config: () => ({ optimizeDeps: { exclude: ['@inkoxide/tailwind'] } }),
    configResolved(config) {
      root = config.root;
      outDir = config.build?.outDir;
    },
    // Each build scans again (Rollup's watch mode builds again); a dev server builds once.
    async buildStart() {
      scanned = undefined;
      const { files } = await start();
      for (const f of files) this.addWatchFile?.(f);
    },
    resolveId(id, _importer, opts) {
      if (id !== '@inkoxide/tailwind') return null;
      const server = opts?.ssr === true || this.environment?.config?.consumer === 'server';
      return server ? null : PRECOMPILED;
    },
    async load(id) {
      if (fileOf(id) !== slashed(TABLE_FILE)) return null;
      return { code: tableModule((await start()).precompiler.table()), map: null };
    },
    async handleHotUpdate({ file, server, read }) {
      if (!SCANNED.has(extname(file)) || scanned === undefined) return undefined;
      const out = outDir === undefined ? undefined : resolve(root, outDir);
      if (!inSources(resolve(file), rootsOf(options, root), out)) return undefined;
      const found = new Set<string>();
      candidatesIn(await read(), found);
      if (!(await scanned).precompiler.add(found)) return undefined;
      // A new class: the table again, and the page, whose styles are cached.
      const mod = server.moduleGraph.getModuleById(slashed(TABLE_FILE));
      if (mod) server.moduleGraph.invalidateModule(mod as never);
      server.ws.send({ type: 'full-reload' });
      return [];
    },
  };
}

/** What the plugin uses of esbuild's plugin API. */
interface EsbuildBuild {
  initialOptions: { platform?: string; absWorkingDir?: string; outdir?: string; outfile?: string };
  onStart(callback: () => void): void;
  onResolve(options: { filter: RegExp }, callback: () => { path: string } | undefined): void;
  onLoad(
    options: { filter: RegExp },
    callback: (args: {
      path: string;
    }) => Promise<{ contents: string; loader: 'js'; watchFiles: string[] } | undefined>,
  ): void;
}

/** An esbuild plugin. */
export interface TailwindEsbuildPlugin {
  name: string;
  setup(build: EsbuildBuild): void;
}

/**
 * The Tailwind build plugin for esbuild: `plugins: [inkoxideTailwindEsbuild()]`. It acts on
 * builds for the browser (esbuild's default platform) and leaves those for Node alone.
 */
export function inkoxideTailwindEsbuild(options: TailwindPluginOptions = {}): TailwindEsbuildPlugin {
  return {
    name: 'inkoxide-tailwind',
    setup(build) {
      const { platform = 'browser', absWorkingDir = process.cwd(), outdir, outfile } = build.initialOptions;
      if (platform !== 'browser') return;
      let scanned: Promise<Scan> | undefined;
      // Each build scans again (in watch mode, and in a context built again).
      build.onStart(() => {
        scanned = undefined;
      });
      build.onResolve({ filter: /^@inkoxide\/tailwind$/ }, () => ({ path: PRECOMPILED }));
      build.onLoad({ filter: /[\\/]table\.[jt]s$/ }, async ({ path }) => {
        if (fileOf(path) !== slashed(TABLE_FILE)) return undefined;
        const s = await (scanned ??= scan(options, absWorkingDir, outdir ?? outfile));
        return { contents: tableModule(s.precompiler.table()), loader: 'js', watchFiles: s.files };
      });
    },
  };
}
