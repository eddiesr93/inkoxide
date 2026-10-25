// Single-file components are compiled by the Vue plugin of vitest.config.ts; tsc sees their default
// export only.
declare module '*.vue' {
  import type { DefineComponent } from 'vue';

  const component: DefineComponent;
  export default component;
}
