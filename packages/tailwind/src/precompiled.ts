import { checkTable, TABLE_GLOBAL, TableSource } from './table-source.ts';
import { TABLE } from './table.ts';
/**
 * `@inkoxide/tailwind` without Tailwind's compiler: classes come from a table of the classes a
 * build saw, which the Tailwind build plugin (`plugin.ts`) writes into `table.ts`. The plugin
 * points `@inkoxide/tailwind` here in client bundles, so a browser need not load the compiler.
 */
import { initSource, type TailwindOptions } from './tw.ts';

export * from './exports.ts';

/**
 * Reads the table of classes once. Safe to call repeatedly. `css`, when given, must be the theme
 * the build used (the plugin's `css` or `cssFile`); `base` is not used.
 */
export async function initTailwind(opts: TailwindOptions = {}): Promise<void> {
  const table = checkTable(TABLE ?? (globalThis as { [TABLE_GLOBAL]?: unknown })[TABLE_GLOBAL]);
  if (opts.css !== undefined && opts.css !== table.css) {
    throw new Error(
      "@inkoxide/tailwind: initTailwind({ css }) has another theme than the one the build used: give the same CSS to the Tailwind build plugin's `css` or `cssFile` option.",
    );
  }
  await initSource(opts, () => Promise.resolve(new TableSource(table)), opts.css !== undefined);
}
