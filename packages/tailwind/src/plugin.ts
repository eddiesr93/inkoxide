/**
 * The Tailwind build plugin: in a browser bundle, `@inkoxide/tailwind` becomes `precompiled.ts`,
 * which reads a table of the classes the sources use instead of loading Tailwind's compiler (about
 * 90 KB with brotli). At the start of a build it scans the sources for classes (`scan.ts`) and
 * builds the table (`table-builder.ts`); a dev server scans each file that changes, and reloads the
 * page when it has a new class. Server bundles keep the compiler.
 *
 * `inkoxideTailwind()` is a Vite plugin, and a Rollup or Rolldown one; `inkoxideTailwindEsbuild()`
 * an esbuild one. No user module is changed: templates resolve their classes at render time, as
 * with the compiler, and get the same styles.
 */
import { readFileSync, realpathSync } from 'node:fs';
import { dirname, extname, isAbsolute, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { candidatesIn, SCANNED, scanFiles, sourceFiles } from './scan.ts';
import { TableBuilder } from './table-builder.ts';
import type { Table } from './table-source.ts';
import type { ThemeOptions } from './tw.ts';

export interface TailwindPluginOptions {
  /** Theme CSS, as for `initTailwind({ css })`; by default the bundled theme. */
  css?: string;
  /** A file with the theme CSS, whose `@import`s resolve from its folder; in place of `css`. */
  cssFile?: string;
  /**
   * Directories and files to scan for classes (relative to the project root); by default the
   * project root, without `node_modules`, build output and hidden directories. A directory's
   * files of the types the scanner knows are read, and a file named here whatever its type.
   */
  sources?: string[];
  /** Classes to keep whether the scan finds them or not: those put together at run time. */
  safelist?: string[];
}

/** The module the plugin puts `@inkoxide/tailwind` in place of, and the table stub it fills. */
const PRECOMPILED = realpathSync(fileURLToPath(new URL('./precompiled.ts', import.meta.url)));
const TABLE_FILE = realpathSync(fileURLToPath(new URL('./table.ts', import.meta.url)));

/** The theme CSS of `options` and where its imports resolve from. */
function theme(options: TailwindPluginOptions, root: string): ThemeOptions {
  if (options.cssFile === undefined) return options.css === undefined ? {} : { css: options.css };
  const file = isAbsolute(options.cssFile) ? options.cssFile : resolve(root, options.cssFile);
  return { css: readFileSync(file, 'utf8'), base: dirname(file) };
}

/** The table stub's code with `table` in it: parsed as JSON, which is faster than a literal. */
function tableModule(table: Table): string {
  return `export const TABLE = JSON.parse(${JSON.stringify(JSON.stringify(table))});\n`;
}

/**
 * The scan of a build's sources, under its root and without its output: the table of their
 * classes, made when first asked for and again after `restart()`, and the files it read.
 */
class SourceScan {
  private readonly options: TailwindPluginOptions;
  private readonly root: string;
  /** The directories and files of `options.sources`, and the build's output, absolute. */
  private readonly roots: string[];
  private readonly out: string | undefined;
  private scanned: Promise<{ builder: TableBuilder; files: string[] }> | undefined;

  constructor(options: TailwindPluginOptions, root: string, out?: string) {
    this.options = options;
    this.root = root;
    this.roots = (options.sources ?? ['.']).map((s) => resolve(root, s));
    this.out = out ? resolve(root, out) : undefined;
  }

  /** Scans again when next asked. */
  restart(): void {
    this.scanned = undefined;
  }

  /** The files the scan read. */
  async files(): Promise<string[]> {
    return (await this.start()).files;
  }

  /** The table stub's code with the table of the classes, and the files they were read from. */
  async table(): Promise<{ code: string; files: string[] }> {
    const { builder, files } = await this.start();
    return { code: tableModule(builder.table()), files };
  }

  /**
   * Adds the classes of `file`, created or changed, when it is one of the sources and the scan
   * has started; true when one was not in the table.
   */
  async update(file: string, read: () => string | Promise<string>): Promise<boolean> {
    if (this.scanned === undefined || !this.has(file)) return false;
    const found = new Set<string>();
    candidatesIn(await read(), found);
    return (await this.scanned).builder.add(found);
  }

  private start() {
    return (this.scanned ??= this.scan());
  }

  private async scan() {
    const builder = await TableBuilder.create(theme(this.options, this.root));
    const files = sourceFiles(this.roots, new Set(this.out === undefined ? [] : [this.out]));
    builder.add(scanFiles(files));
    builder.add(this.options.safelist ?? []);
    return { builder, files };
  }

  /**
   * Whether `file` is one of the sources: one of the roots, or of a type the scanner reads in one
   * of them, and not in the output.
   */
  private has(file: string): boolean {
    const under = (dir: string) => file === dir || file.startsWith(dir + sep);
    if (this.out !== undefined && under(this.out)) return false;
    return this.roots.includes(file) || (SCANNED.has(extname(file)) && this.roots.some(under));
  }
}

/** A path with `/` between its parts, as bundlers write module ids. */
const slashed = (path: string): string => path.split(sep).join('/');

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

/** What the plugin uses of a Vite dev server's environment. */
interface DevEnvironment {
  config: { consumer: string };
  moduleGraph: {
    getModuleById(id: string): unknown;
    invalidateModule(mod: never): void;
  };
  hot: { send(payload: { type: 'full-reload' }): void };
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
  hotUpdate(
    this: { environment: DevEnvironment },
    ctx: { type: 'create' | 'update' | 'delete'; file: string; read(): string | Promise<string> },
  ): Promise<[] | undefined>;
}

/**
 * The Tailwind build plugin for Vite, Rollup and Rolldown:
 * `plugins: [inkoxideTailwind()]` in `vite.config.ts`. Server builds (Vite's SSR) keep the
 * compiler.
 */
export function inkoxideTailwind(options: TailwindPluginOptions = {}): TailwindPlugin {
  let sources = new SourceScan(options, process.cwd());
  return {
    name: 'inkoxide-tailwind',
    enforce: 'pre',
    // Vite would bundle the package ahead of time, around this plugin.
    config: () => ({ optimizeDeps: { exclude: ['@inkoxide/tailwind'] } }),
    configResolved(config) {
      sources = new SourceScan(options, config.root, config.build?.outDir);
    },
    // Each build scans again (Rollup's watch mode builds again); a dev server builds once.
    async buildStart() {
      sources.restart();
      for (const f of await sources.files()) this.addWatchFile?.(f);
    },
    resolveId(id, _importer, opts) {
      if (id !== '@inkoxide/tailwind') return null;
      const server = opts?.ssr === true || this.environment?.config?.consumer === 'server';
      return server ? null : PRECOMPILED;
    },
    async load(id) {
      if (fileOf(id) !== slashed(TABLE_FILE)) return null;
      return { code: (await sources.table()).code, map: null };
    },
    // A file created or changed in the browser's environment (only it has the table; a removed
    // file's classes stay until the next build).
    async hotUpdate({ type, file, read }) {
      const env = this.environment;
      if (type === 'delete' || env.config.consumer !== 'client') return undefined;
      if (!(await sources.update(resolve(file), read))) return undefined;
      // A new class: the table again, and the page, whose styles are cached.
      const mod = env.moduleGraph.getModuleById(slashed(TABLE_FILE));
      if (mod) env.moduleGraph.invalidateModule(mod as never);
      env.hot.send({ type: 'full-reload' });
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
      const sources = new SourceScan(options, absWorkingDir, outdir ?? outfile);
      // Each build scans again (in watch mode, and in a context built again).
      build.onStart(() => sources.restart());
      build.onResolve({ filter: /^@inkoxide\/tailwind$/ }, () => ({ path: PRECOMPILED }));
      build.onLoad({ filter: /[\\/]table\.[jt]s$/ }, async ({ path }) => {
        if (fileOf(path) !== slashed(TABLE_FILE)) return undefined;
        const { code, files } = await sources.table();
        return { contents: code, loader: 'js', watchFiles: files };
      });
    },
  };
}
