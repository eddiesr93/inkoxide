// The Vite plugin of `@inkoxide/vue/vite`: `@vitejs/plugin-vue`, compiling the PDF templates for
// the client in a server build too.

type Hook = (this: unknown, ...args: unknown[]) => unknown;
type HookOrObject = Hook | { handler: Hook };

/** The parts of a Vite plugin this function changes: `@vitejs/plugin-vue`'s. */
export interface VuePluginLike {
  name: string;
  load?: unknown;
  transform?: unknown;
}

/** Which files are PDF templates: a test on their path, or a regular expression. */
export type TemplateFilter = RegExp | ((file: string) => boolean);

/**
 * `vuePlugin` (`@vitejs/plugin-vue`'s), compiling the files `include` matches (every `.vue` file
 * when it is left out) for the client, even in a server build. Use it in place of the plugin, or
 * as a second instance of it in a config of its own for the templates.
 */
export function pdfTemplates<P extends VuePluginLike>(vuePlugin: P, include?: TemplateFilter): P {
  const matches = (id: string): boolean => {
    const file = id.split('?', 1)[0] ?? id;
    if (include === undefined) return true;
    return typeof include === 'function' ? include(file) : include.test(file);
  };
  const wrap = (hook: unknown, at: number): unknown => {
    if (hook === undefined) return undefined;
    const fn = (typeof hook === 'function' ? hook : (hook as { handler: Hook }).handler) as Hook;
    const handler: Hook = function (this: unknown, ...args: unknown[]) {
      if (matches(String(args[0]))) args[at] = { ...(args[at] as object | undefined), ssr: false };
      return fn.apply(this, args);
    };
    return typeof hook === 'function' ? handler : { ...(hook as object), handler };
  };
  return {
    ...vuePlugin,
    load: wrap(vuePlugin.load as HookOrObject | undefined, 1),
    transform: wrap(vuePlugin.transform as HookOrObject | undefined, 2),
  };
}
