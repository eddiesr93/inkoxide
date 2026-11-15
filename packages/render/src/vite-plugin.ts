/**
 * The Vite plugin of every front end (`@inkoxide/react/vite` and the others): the configuration a
 * page that renders in the browser needs, in place of writing it by hand. It imports nothing of
 * the engine, so a config file loads it without starting one.
 */

/** What the plugin sets in Vite's configuration. */
interface BrowserConfig {
  worker: { format: 'es' };
  optimizeDeps: { exclude: string[] };
}

/** The plugin `inkoxide()` makes: a Vite plugin. */
export interface InkoxidePlugin {
  name: string;
  config(): BrowserConfig;
}

/**
 * `plugins: [inkoxide()]` in `vite.config.ts` sets what the engine needs in a browser:
 *
 * - `worker.format: 'es'`: the engine's worker is a module worker;
 * - `optimizeDeps.exclude: ['@inkoxide/wasm']`: the dev server would bundle the module away from
 *   its `hyphenation/` folder, and a language other than English would not hyphenate.
 */
export function inkoxide(): InkoxidePlugin {
  return {
    name: 'inkoxide',
    config: () => ({ worker: { format: 'es' }, optimizeDeps: { exclude: ['@inkoxide/wasm'] } }),
  };
}
