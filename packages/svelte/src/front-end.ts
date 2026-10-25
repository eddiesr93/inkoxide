import type { FrontEnd } from '@inkoxide/render';
import type { TreeOptions, TreeResult } from '@inkoxide/tree';
import type { Component } from 'svelte';

import { componentName, type Renderable } from './template.ts';

/** Svelte as a front end of `@inkoxide/render`, with the template renderer of where it runs. */
export function svelteFrontEnd(
  renderTemplate: (input: Renderable, opts: TreeOptions) => TreeResult,
  // oxlint-disable-next-line typescript/no-explicit-any -- any component, whatever its props.
): FrontEnd<Renderable, Component<any>> {
  return {
    tree: renderTemplate,
    withData: (component, data) => ({ component, props: { data } }),
    name: componentName,
  };
}
