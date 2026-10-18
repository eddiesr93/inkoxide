/**
 * What Tailwind writes for a class, as its compiler (`compile.ts`) and a table a build made
 * (`table-source.ts`) both give it to the mapper (`parse.ts`).
 */
import type { MediaQuery } from '@inkoxide/style/internal';

/**
 * A declaration, `[property, value, media]`: `media` are the page media it is for (`md:`,
 * `landscape:`), all of which must match; none for most.
 */
export type Decl = readonly [property: string, value: string, media: readonly MediaQuery[]];

/** What Tailwind emits for one class, before any value is resolved. */
export interface CandidateCss {
  /** Declarations in source order; custom properties included. */
  decls: Decl[];
  /** `@property --tw-*` initial values the rule depends on. */
  initials: Array<[string, string]>;
}

/** Why a class cannot be used, when Tailwind's output already says so. */
export type CandidateProblem =
  | { kind: 'unknown' }
  /** Not in the table a build made (`precompiled.ts`): unknown, or not seen. */
  | { kind: 'unseen' }
  /** A media query around the rule that does not parse or reads the screen: why, quoting it. */
  | { kind: 'media'; message: string }
  /** An at-rule other than `@media` and `@supports` (`@container …`). */
  | { kind: 'atRule'; rule: string }
  /** `@media` inside the rule: sizes that follow the screen's (`container`). */
  | { kind: 'screenSizes'; rule: string }
  /** A selector beyond the class: a state (`&:hover`), a part (`::marker`), other elements. */
  | { kind: 'selector'; selector: string };

export type CandidateResult = { ok: true; css: CandidateCss } | { ok: false; problem: CandidateProblem };
