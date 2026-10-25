/**
 * A component of this package rendered by Svelte alone. In a file of its own: Svelte before 5.2x
 * keeps the contexts of a server render that threw, which the next render then sees.
 */
import { render } from 'svelte/server';
import { expect, it } from 'vitest';

import View from '../src/components/View.svelte';

it('says that a component renders only inside an Inkoxide render', () => {
  expect(() => render(View).body).toThrow(/<view> renders only inside an Inkoxide render/);
});
