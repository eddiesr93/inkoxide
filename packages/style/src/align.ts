// `verticalAlign`, as the style prop and Tailwind's `align-*` classes write it.
import { RUN_ALIGNS } from './types.ts';

/** `verticalAlign`'s keywords: a cell's, then a run's, then those that place an image in a line. */
export const VERTICAL_ALIGNS = ['top', 'middle', 'bottom', ...RUN_ALIGNS, 'textTop', 'textBottom'] as const;
/** A `verticalAlign` keyword. */
export type VerticalAlignKeyword = (typeof VERTICAL_ALIGNS)[number];

/**
 * A `verticalAlign` keyword, CSS's `text-top` and `text-bottom` read as `textTop` and
 * `textBottom`; undefined for any other value.
 */
export function verticalAlignKeyword(value: string): VerticalAlignKeyword | undefined {
  const camel = value === 'text-top' ? 'textTop' : value === 'text-bottom' ? 'textBottom' : value;
  return (VERTICAL_ALIGNS as readonly string[]).includes(camel) ? (camel as VerticalAlignKeyword) : undefined;
}
