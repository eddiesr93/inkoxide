// The components are compiled by Svelte's tools; tsc sees their default export only.
declare module '*.svelte' {
  import type { Component } from 'svelte';

  // oxlint-disable-next-line typescript/no-explicit-any -- whatever props the component takes.
  const component: Component<any>;
  export default component;
}
