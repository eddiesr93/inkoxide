import { DEFAULT_SANS } from '@inkoxide/style/internal';

/** A font family as a CSS value: quoted when it has a space. */
const cssFamily = (family: string) => (/\s/.test(family) ? `"${family}"` : family);

/**
 * The default theme: Tailwind's, with font stacks that name the families registered with the
 * engine (FontRegistry in @inkoxide/react) and a small document palette. A project can pass its
 * own CSS to initTailwind({ css }); keep these families in it.
 *
 * A string rather than a .css file so it survives bundling.
 */
export const DEFAULT_THEME_CSS = `@import "tailwindcss/theme";

@theme {
  --font-sans: ${DEFAULT_SANS.map(cssFamily).join(', ')};
  --font-serif: "Noto Sans";
  --font-mono: "Roboto Mono";
  --font-display: "Inter Display", Inter;

  /* A small document palette (sRGB percentages, as in the engine fixtures). */
  --color-ink: rgb(9% 10% 13%);
  --color-ink-muted: rgb(40% 42% 48%);
  --color-brand: rgb(12% 29% 60%);
  --color-field: rgb(97% 97.5% 98.5%);
  --color-field-line: rgb(55% 57% 62%);
  --color-rule: rgb(85% 86% 90%);
  --color-head: rgb(93% 95% 99%);
}
`;
