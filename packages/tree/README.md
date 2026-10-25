# @inkoxide/tree

What every Inkoxide front end shares: the components' host types and the types of their props,
where each component may go, and the conversion of the host nodes a front end renders into an
Inkoxide document tree, with fonts, images, Tailwind classes and errors that name the component
at fault.

To make PDFs, install the package of your framework instead; each brings this one with it:
[`@inkoxide/react`](https://www.npmjs.com/package/@inkoxide/react),
[`@inkoxide/vue`](https://www.npmjs.com/package/@inkoxide/vue),
[`@inkoxide/svelte`](https://www.npmjs.com/package/@inkoxide/svelte) or
[`@inkoxide/angular`](https://www.npmjs.com/package/@inkoxide/angular).

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later; ES
modules only.

```sh
npm install @inkoxide/tree
```

## Writing a front end

A front end renders a template into host nodes: plain objects, an element for each component
(`{ kind: 'element', type: 'ink-view', props, children }`) and a text node for each string. Then
`buildDocument` makes the document:

```js
import { buildDocument, checkTree, FontRegistry, HOST } from '@inkoxide/tree';

const fonts = new FontRegistry();
fonts.register({ id: 'Inter-Regular', family: 'Inter' });

const container = {
  children: [
    { kind: 'element', type: HOST.Document, props: {}, children: [
      { kind: 'element', type: HOST.Page, props: { size: 'A4' }, children: [
        { kind: 'element', type: HOST.Text, props: { style: { fontSize: 18 } }, children: [
          { kind: 'text', text: 'Hello' },
        ] },
      ] },
    ] },
  ],
};
checkTree(container); // where each component may go
const { document } = buildDocument(container, {}, { fonts });
```

A node may keep an `origin` and a `trace` that names the components above it, for the component
stacks of errors. A front end that supports `render` props passes `renderPage`, which renders an
element's `render` prop for one page. `trimTemplateText` gives the strings of template languages
the white space JSX would, and `TAGS` and `BOOLEAN_PROPS` serve front ends that write elements and
attributes. [`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render)'s
`createRenderer` turns a front end into the functions that render PDFs.

## Licence

MIT
