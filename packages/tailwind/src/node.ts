/**
 * The design system loader in Node: `@tailwindcss/node`, which reads a theme's `@import`s from
 * disk. Importing it installs it.
 */
import { createRequire } from 'node:module';
import { dirname } from 'node:path';

import { __unstable__loadDesignSystem } from '@tailwindcss/node';

import { setDesignSystemLoader } from './compile.ts';

/** Where `@import "tailwindcss/…"` resolves from: the tailwindcss package itself. */
const TAILWIND_DIR = dirname(createRequire(import.meta.url).resolve('tailwindcss/package.json'));

setDesignSystemLoader((css, base = TAILWIND_DIR) => __unstable__loadDesignSystem(css, { base }));
