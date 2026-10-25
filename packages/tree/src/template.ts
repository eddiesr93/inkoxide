// What every front end does around its own render: the error of a template that threw, the
// reading of what it rendered, and the two functions a template becomes a document tree with.
import type { Document } from '@inkoxide/schema';

import type { HostType } from './components/index.ts';
import { TemplateError } from './errors.ts';
import { type BuildHooks, buildDocument, type TreeOptions, type TreeResult } from './index.ts';
import { type Container, type Node, trimTemplateText } from './nodes.ts';
import { checkTree } from './placement.ts';
import { loadTailwind } from './tailwind.ts';

/**
 * The TemplateError of what a template threw: a TemplateError as it is (given the stack when it
 * has none), anything else as a `render` error that carries it.
 */
export function templateError(error: unknown, componentStack?: string): TemplateError {
  if (error instanceof TemplateError) {
    error.componentStack ??= componentStack;
    return error;
  }
  const message = error instanceof Error ? error.message : String(error);
  return new TemplateError(`The template threw: ${message}`, {
    kind: 'render',
    componentStack,
    cause: error,
  });
}

/**
 * The container of the nodes a template language rendered: their text as JSX gives it
 * (`trimTemplateText`), checked for where each component is. `parent` is the host type a `render`
 * prop's content renders into.
 */
export function finishTemplate(nodes: Node[], parent?: HostType): Container {
  const container: Container = parent === undefined ? { children: [] } : { children: [], parent };
  container.children = trimTemplateText(nodes, parent);
  checkTree(container);
  return container;
}

/** A front end's template functions; `Input` is what its users render. */
export interface TemplateFunctions<Input> {
  /**
   * Renders a template to an Inkoxide document. Synchronous: the template renders once, then its
   * host nodes are walked into the schema tree. Throws TemplateError with a component stack.
   */
  renderTemplate(input: Input, opts?: TreeOptions): TreeResult;
  /**
   * The document tree of a template (the simple API). Loads Tailwind first when it is installed.
   * A template error is a rejected promise, not a throw.
   */
  renderToTree(input: Input, opts?: TreeOptions): Promise<Document>;
}

/** The template functions of a front end that renders an input with `mount`. */
export function templateFunctions<Input>(
  mount: (input: Input) => Container,
  hooks: BuildHooks,
): TemplateFunctions<Input> {
  const renderTemplate = (input: Input, opts: TreeOptions = {}): TreeResult => {
    const t0 = performance.now();
    const container = mount(input);
    return buildDocument(container, hooks, opts, performance.now() - t0);
  };
  return {
    renderTemplate,
    async renderToTree(input, opts = {}) {
      await loadTailwind();
      return renderTemplate(input, opts).document;
    },
  };
}
