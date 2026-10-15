// `@inkoxide/tailwind` in Node: a theme's `@import`s are read from disk (node.ts). A browser
// gets browser.ts instead, through the package's `browser` export condition.
import './node.ts';

export * from './api.ts';
