/**
 * The design system loader in a browser: Tailwind's own, with its theme bundled as text, since
 * nothing can be read from disk. Importing it installs it.
 */
import { __unstable__loadDesignSystem } from 'tailwindcss';

import { setDesignSystemLoader } from './compile.ts';
import { TAILWIND_THEME_CSS } from './tailwind-theme.ts';

const THEME = new Set(['tailwindcss/theme', 'tailwindcss/theme.css']);

setDesignSystemLoader((css) =>
  __unstable__loadDesignSystem(css, {
    base: '/',
    loadStylesheet: async (id, base) => {
      if (THEME.has(id)) return { path: 'tailwindcss/theme.css', base, content: TAILWIND_THEME_CSS };
      throw new Error(
        `The theme imports "${id}", but in a browser a theme can import only "tailwindcss/theme": put the rest of its CSS in the theme itself.`,
      );
    },
  }),
);
