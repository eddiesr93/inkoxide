import type { PageInfo } from '@inkoxide/tree';
import type { Component, Snippet } from 'svelte';

/** The root of a `render` snippet's content for one page. */
declare const PageRoot: Component<{ snippet: Snippet<[PageInfo]>; page: PageInfo }>;
export default PageRoot;
