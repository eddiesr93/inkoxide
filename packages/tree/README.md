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
const tree = buildDocument(container, {}, { fonts });
tree.document; // the document, for the engine
tree.dispose(); // when done with it: the front end releases what it rendered
```

A node may keep an `origin` and a `trace` that names the components above it, for the component
stacks of errors. A container may keep `dispose`, which releases what the front end rendered into
it (an app, a component); the document's `dispose` calls it, and that of each page root. A front
end that supports `render` props passes `renderPage` (in `BuildHooks`), which renders an element's
`render` prop for one page into a root of its own. `templateFunctions(mount, hooks)` gives a front
end its `renderTemplate` and `renderToTree`; `finishTemplate` reads what a template language
rendered (its text as JSX would give it, checked), and `templateError` turns what a template threw
into a `TemplateError`. `COMPONENTS`, `TAGS`, `tagName` and `BOOLEAN_PROPS` serve front ends that
write elements and attributes, `PropsByComponent` types each component's props by its name, and
`@inkoxide/tree/props` has the types of the props every front end exports.
[`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render)'s `createRenderer` turns a
front end into the functions that render PDFs.

## API

Every name the package exports; test/exports.test.ts pins the list. `@inkoxide/tree/props` has
the props and style types besides.

| Names | What they are |
|---|---|
| `buildDocument`, `BuildHooks`, `RenderPage`, `TreeOptions`, `TreeResult`, `Located` | Host nodes to a document, with `locate` (an engine path to its component), `validate`, `rebuild` (for another page count) and `dispose`. |
| `assertValidTree`, `validateTree`, `SchemaProblem` | The document checked against the engine's schema: the first problem as a `TemplateError`, or them all. |
| `Container`, `Instance`, `TextInstance`, `Node`, `Trace` | Host nodes: an element of each component, a text node of each string. |
| `checkTree`, `checkElement`, `checkText`, `rootContext`, `childContext`, `HostContext` | Where each component may go: a tree checked whole, or an element or a string as a renderer inserts it. |
| `componentNames`, `componentStack`, `formatStack`, `innermostComponent`, `dedupe`, `hostName`, `isHostType` | Component stacks (`\n    in Text\n    in Invoice`), from the nodes' traces. |
| `templateFunctions`, `TemplateFunctions`, `finishTemplate`, `trimTemplateText` | What a front end does around its render: the render's functions, and the reading of what a template language rendered. |
| `TemplateError`, `TemplateErrorInit`, `TemplateErrorKind`, `templateError` | The error of a problem in a template, with its component stack. |
| `renderWithTrace`, `namesInTrace` | For template languages that keep no component stack: a render with a long trace, and the component names in it. |
| `assignProp`, `classNames`, `nodeLinks` | For renderers that set attributes and insert nodes: a prop as every template front end reads it (`class`, a bare attribute), and the links of nodes to their parents. |
| `HOST`, `HOST_NAME`, `HostType`, `COMPONENTS`, `FIELD_KINDS`, `TAGS`, `tagName`, `BOOLEAN_PROPS` | The components: their host types and names, the field components' kinds, their element names (`ink-view`), and the props that take `true`. |
| `PropsByComponent`, `RenderProp`, `Common`, `Flow`, `FieldCommon` | The props of each component by its name, and the props several components share. |
| `FontRegistry`, `fonts`, `FontFace`, `ResolvedFont`, `EmojiSource`, `familyKey`, `fontWeightValue`, `fontStretchValue`, `fontVariationsValue` | Fonts: the registry (`fonts` is the process-wide one), and how a family name, a weight, a stretch and variations are read. |
| `imageSource`, `ImageSource`, `isImagePath`, `PendingImage`, `RemoteImagePolicy`, `remoteAllowed`, `remoteRefused` | Images: how an `src` is read (an id, a path, a URL, bytes), and the policy for remote ones. |
| `loadTailwind`, `StyleSheet` | Styles: Tailwind's design system, loaded when installed, and `StyleSheet.create`. |
| `facturX`, `facturXRelationship`, `FacturXOptions`, `FacturXProfile` | Factur-X and ZUGFeRD invoices: the XML attached as the profile requires. |
| `PAGE_SIZES`, `parsePath`, `OutputOptions`, `EncryptionOptions`, `PdfALevel`, `PdfUaPart` | Page sizes by name, an engine path split into keys, and the options a `<Document>` gives the engine. |
| `Painter`, `DrawingFont`, `CanvasGradient`, `CanvasTextOptions` | The painter of `<Canvas paint>`. |

## Licence

MIT
