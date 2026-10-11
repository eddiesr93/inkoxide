/** What went wrong while turning a template into a document tree. */
export type TemplateErrorKind =
  /** A component in the wrong place (`<View>` inside `<Text>`, a string inside `<Table>`). */
  | 'structure'
  /** An exception thrown by template code. */
  | 'render'
  /** A class the Tailwind mapper rejects. */
  | 'tailwind'
  /** A prop with an invalid value (bad page size, remote image URL, …). */
  | 'props'
  /** The built tree does not match the engine's JSON Schema (dev only). */
  | 'schema';

export interface TemplateErrorInit {
  kind?: TemplateErrorKind;
  componentStack?: string;
  className?: string;
  path?: string;
  cause?: unknown;
}

/**
 * An error in a template, with the React component stack that leads to the offending element
 * and, for Tailwind problems, the class string.
 */
export class TemplateError extends Error {
  override readonly name = 'TemplateError';
  kind: TemplateErrorKind;
  componentStack: string | undefined;
  className: string | undefined;
  /** The engine path of the node, when known (`pages[0].children[3]`). */
  path: string | undefined;

  constructor(message: string, init: TemplateErrorInit = {}) {
    super(message, init.cause === undefined ? undefined : { cause: init.cause });
    this.kind = init.kind ?? 'structure';
    this.componentStack = init.componentStack;
    this.className = init.className;
    this.path = init.path;
  }

  /** The message followed by the component stack, as React prints it. */
  get detail(): string {
    return `${this.message}${this.componentStack ?? ''}`;
  }
}
