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
import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { candidatesIn, matchesGlob, SCANNED, scanFiles, type SourceGlob, sourceFiles } from './scan.ts';
import { themeSources } from './sources.ts';
import { TableBuilder } from './table-builder.ts';
import type { Table } from './table-source.ts';
import type { ThemeOptions } from './tw.ts';
import { webpackScans } from './webpack-scans.ts';

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

/** The theme CSS of `options`, where its imports resolve from, and its file. */
function themeOf(options: TailwindPluginOptions, root: string): ThemeOptions & { file?: string } {
  if (options.cssFile === undefined) return options.css === undefined ? {} : { css: options.css };
  const file = isAbsolute(options.cssFile) ? options.cssFile : resolve(root, options.cssFile);
  return { css: readFileSync(file, 'utf8'), base: dirname(file), file };
}

/** The table stub's code with `table` in it: parsed as JSON, which is faster than a literal. */
function tableModule(table: Table): string {
  return `export const TABLE = JSON.parse(${JSON.stringify(JSON.stringify(table))});\n`;
}

/** What a scan read, and what it takes for one of the sources. */
interface Scanned {
  builder: TableBuilder;
  /** The sources read, and the theme's CSS files. */
  files: string[];
  theme: string[];
  /** Whether `file`, created or changed, is one of the sources. */
  has(file: string): boolean;
  /** Classes the theme never keeps (`@source not inline`). */
  never: ReadonlySet<string>;
}

/**
 * The scan of a build's sources, under its root and without its output: the table of their
 * classes, made when first asked for and again after `restart()`, and the files it read. The
 * theme's `@source` directives add to the sources (`sources.ts`).
 */
class SourceScan {
  private readonly options: TailwindPluginOptions;
  private readonly root: string;
  private readonly out: string | undefined;
  private scanned: Promise<Scanned> | undefined;

  constructor(options: TailwindPluginOptions, root: string, out?: string) {
    this.options = options;
    this.root = root;
    this.out = out ? resolve(root, out) : undefined;
  }

  /** Scans again when next asked. */
  restart(): void {
    this.scanned = undefined;
  }

  /** The files the scan read: the sources, and the theme's CSS files. */
  async files(): Promise<string[]> {
    const { files, theme } = await this.start();
    return [...files, ...theme];
  }

  /** The table stub's code with the table of the classes, and the files they were read from. */
  async table(): Promise<{ code: string; files: string[] }> {
    const { builder } = await this.start();
    return { code: tableModule(builder.table()), files: await this.files() };
  }

  /** Whether `file` is one of the theme's CSS files, once scanned: a change needs a new scan. */
  async isTheme(file: string): Promise<boolean> {
    return this.scanned !== undefined && (await this.scanned).theme.includes(file);
  }

  /**
   * Adds the classes of `file`, created or changed, when it is one of the sources and the scan
   * has started; true when one was not in the table.
   */
  async update(file: string, read: () => string | Promise<string>): Promise<boolean> {
    if (this.scanned === undefined) return false;
    const scanned = await this.scanned;
    if (!scanned.has(file)) return false;
    const found = new Set<string>();
    candidatesIn(await read(), found);
    for (const c of scanned.never) found.delete(c);
    return scanned.builder.add(found);
  }

  private start() {
    return (this.scanned ??= this.scan());
  }

  private async scan(): Promise<Scanned> {
    const theme = themeOf(this.options, this.root);
    const builder = await TableBuilder.create(theme);
    const named =
      theme.css === undefined ? undefined : await themeSources(theme.css, theme.base ?? this.root);
    // The plugin's `sources`, else where the theme starts the scan, else the project root.
    const roots: (string | SourceGlob)[] = [
      ...(this.options.sources?.map((s) => resolve(this.root, s)) ?? named?.root ?? [this.root]),
      ...(named?.include ?? []),
    ];
    const exclude = new Set([...(this.out === undefined ? [] : [this.out]), ...(named?.exclude ?? [])]);
    const excludeGlobs = named?.excludeGlobs ?? [];
    const never = named?.notInline ?? new Set<string>();
    const files = sourceFiles(roots, exclude, excludeGlobs);
    const keep = (classes: Iterable<string>) => [...classes].filter((c) => !never.has(c));
    builder.add(keep(scanFiles(files)));
    builder.add(keep([...(this.options.safelist ?? []), ...(named?.inline ?? [])]));
    const themeFiles = [...(theme.file === undefined ? [] : [theme.file]), ...(named?.files ?? [])];
    const under = (file: string, dir: string) => file === dir || file.startsWith(dir + sep);
    const has = (file: string) => {
      if ([...exclude].some((d) => under(file, d))) return false;
      if (excludeGlobs.some((g) => matchesGlob(file, g))) return false;
      return roots.some((r) =>
        typeof r === 'string'
          ? r === file || (SCANNED.has(extname(file)) && under(file, r))
          : matchesGlob(file, r),
      );
    };
    return { builder, files, theme: themeFiles, has, never };
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
    // file's classes stay until the next build), or the theme.
    async hotUpdate({ type, file, read }) {
      const env = this.environment;
      if (type === 'delete' || env.config.consumer !== 'client') return undefined;
      const path = resolve(file);
      // The theme, which may name other sources: a new scan.
      if (await sources.isTheme(path)) sources.restart();
      else if (!(await sources.update(path, read))) return undefined;
      // A new class or theme: the table again, and the page, whose styles are cached.
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

/** The loader that fills the table stub in webpack and Rspack builds. */
const LOADER = fileURLToPath(new URL('./webpack-loader.ts', import.meta.url));

/** What the plugin uses of a webpack or Rspack compiler. */
interface WebpackCompiler {
  context: string;
  options: {
    target?: unknown;
    output?: { path?: string };
    resolve?: { alias?: unknown };
    module?: { rules?: unknown[] };
  };
  hooks: {
    watchRun: { tap(name: string, fn: () => void): void };
    run: { tap(name: string, fn: () => void): void };
  };
}

/** A webpack or Rspack plugin. */
export interface TailwindWebpackPlugin {
  apply(compiler: WebpackCompiler): void;
}

/** Whether a webpack target is for Node (or Electron's main process), not a browser. */
function nodeTarget(target: unknown): boolean {
  const targets = Array.isArray(target) ? target : [target];
  return targets.some((t) => typeof t === 'string' && /^(?:node|async-node|electron-main)/.test(t));
}

/** Escapes `s` for a regular expression. */
const literal = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * The Tailwind build plugin for webpack and Rspack: `plugins: [inkoxideTailwindWebpack()]`. It
 * acts on builds for the browser and leaves those whose `target` is Node alone. Each build scans
 * again; in watch mode, each rebuild.
 */
export function inkoxideTailwindWebpack(options: TailwindPluginOptions = {}): TailwindWebpackPlugin {
  const key = randomUUID();
  return {
    apply(compiler) {
      if (nodeTarget(compiler.options.target)) return;
      const sources = new SourceScan(options, compiler.context, compiler.options.output?.path);
      webpackScans().set(key, sources);
      const restart = () => sources.restart();
      compiler.hooks.run.tap('inkoxide-tailwind', restart);
      compiler.hooks.watchRun.tap('inkoxide-tailwind', restart);
      const resolveOptions = (compiler.options.resolve ??= {});
      // `$`: `@inkoxide/tailwind` itself, not its other entries.
      const alias = { '@inkoxide/tailwind$': PRECOMPILED };
      if (Array.isArray(resolveOptions.alias)) {
        resolveOptions.alias.push({ name: '@inkoxide/tailwind', alias: PRECOMPILED, onlyModule: true });
      } else {
        resolveOptions.alias = { ...(resolveOptions.alias as object | undefined), ...alias };
      }
      const module = (compiler.options.module ??= {});
      (module.rules ??= []).push({
        test: new RegExp(`^${literal(TABLE_FILE)}$`),
        use: [{ loader: LOADER, options: { scan: key } }],
      });
    },
  };
}

/** What the wrapper uses of a Next.js config. */
export interface NextConfig {
  webpack?: (config: WebpackConfig, context: { isServer: boolean; dir: string }) => WebpackConfig;
  turbopack?: { resolveAlias?: Record<string, unknown>; [key: string]: unknown };
  [key: string]: unknown;
}

/** What the wrapper uses of the webpack config Next.js makes. */
interface WebpackConfig {
  plugins?: unknown[];
  [key: string]: unknown;
}

type NextConfigFunction = (
  phase: string,
  context: { defaultConfig: NextConfig },
) => NextConfig | Promise<NextConfig>;

/** The phases that bundle: `next dev` and `next build`. */
const BUNDLING = new Set(['phase-development-server', 'phase-production-build']);

/** The module Turbopack bundles in place of `@inkoxide/tailwind` in the browser, under `root`. */
const NEXT_MODULE = 'node_modules/.cache/@inkoxide/tailwind/browser.js';

/**
 * Writes the module Turbopack bundles in place of `@inkoxide/tailwind` in the browser, when it
 * changed: the table of `sources`, and `precompiled.ts`. That goes by its path: the alias takes
 * `@inkoxide/tailwind/…` too.
 */
async function writeNextModule(sources: SourceScan, root: string): Promise<void> {
  const { code } = await sources.table();
  const table = code.slice('export const TABLE = '.length).trimEnd().replace(/;$/, '');
  const file = join(root, NEXT_MODULE);
  const precompiled = relative(dirname(file), PRECOMPILED).split(sep).join('/');
  const text = `// The Tailwind build plugin's module (withInkoxideTailwind()): the table of the classes the sources use.
globalThis[Symbol.for('@inkoxide/tailwind table')] = ${table};
export * from ${JSON.stringify(precompiled)};
`;
  try {
    if (readFileSync(file, 'utf8') === text) return;
  } catch {
    // Not written yet.
  }
  mkdirSync(dirname(file), { recursive: true });
  // Its own package: an app whose package.json says `"sideEffects": false` would drop the table.
  writeFileSync(join(dirname(file), 'package.json'), '{ "type": "module", "sideEffects": true }\n');
  writeFileSync(file, text);
}

/**
 * The Tailwind build plugin for Next.js: `export default withInkoxideTailwind(nextConfig)` in
 * `next.config.ts`, with the config as an object or a function. Browser bundles get the table,
 * with Turbopack and with webpack; server bundles keep the compiler. Turbopack has no hook for a
 * dev server: `next dev` scans when it starts, and a class new since then needs a restart.
 */
export function withInkoxideTailwind(
  config: NextConfig | NextConfigFunction = {},
  options: TailwindPluginOptions = {},
): NextConfigFunction {
  return async (phase, context) => {
    const user = typeof config === 'function' ? await config(phase, context) : config;
    const root = process.cwd();
    if (BUNDLING.has(phase)) await writeNextModule(new SourceScan(options, root), root);
    return {
      ...user,
      turbopack: {
        ...user.turbopack,
        resolveAlias: {
          ...user.turbopack?.resolveAlias,
          '@inkoxide/tailwind': { browser: `./${NEXT_MODULE}` },
        },
      },
      webpack(webpackConfig, ctx) {
        const out = user.webpack ? user.webpack(webpackConfig, ctx) : webpackConfig;
        if (!ctx.isServer) (out.plugins ??= []).push(inkoxideTailwindWebpack(options));
        return out;
      },
    };
  };
}
