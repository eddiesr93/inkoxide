// Component stacks in Svelte, which keeps none: each component is the function of its file
// (`Invoice.svelte`), so a trace names the components it passes through.
import { formatStack, namesInTrace } from '@inkoxide/tree';

/** A `.svelte` file in a trace (with the query a dev server adds), and its name. */
const FILE = /([^/\\\s()]+)\.svelte(?:\?[^:\s)]*)?:\d+/g;

/** The components a trace names, innermost first; empty when the code lost its file names. */
export const svelteNames = (error: unknown): string[] =>
  namesInTrace(error, FILE, (name) => name !== 'PageRoot');

/** The component stack of a Svelte error, if its trace names components. */
export function svelteStack(error: unknown): string | undefined {
  const names = svelteNames(error);
  return names.length ? formatStack(names) : undefined;
}
