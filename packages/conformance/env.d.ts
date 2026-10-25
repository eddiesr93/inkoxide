// Single-file components are compiled by the Vue plugin of vitest.config.ts; tsc sees their default
// export only.
declare module '*.vue' {
  import type { DefineComponent } from 'vue';

  const component: DefineComponent;
  export default component;
}

// Svelte components are compiled by the Svelte plugin of vitest.config.ts.
declare module '*.svelte' {
  import type { Component } from 'svelte';

  // oxlint-disable-next-line typescript/no-explicit-any -- whatever props the component takes.
  const component: Component<any>;
  export default component;
}
