# 4. Front ends for React, Vue, Svelte and Angular

Status: accepted, 2026-10-25; for 1.0.0.

## Context

Up to 0.10, templates were React components only. `@inkoxide/react` rendered them with a
reconciler into plain host nodes (a type, props and children per component, and the strings),
and walked those into the document tree; `@inkoxide/render` re-exported the components and
rendered React elements to PDF. React appeared in four places of the walk: the reconciler, the
fiber each node kept for the component stack of errors, the `render` prop (rendered again for each
page, as a React root of its own), and the components, which were host types cast to React
components. Everything else, the builder included, only read host nodes.

Each framework renders differently:

- Vue has a public custom renderer API, `createRenderer`, whose node operations can build any
  objects. Its `patchProp` receives the component instance that rendered an element.
- Angular renders through a `RendererFactory2`. `createComponent` takes it from the injector chain
  (`createRootLViewEnvironment` in 22.2.3), so a factory given in the injector receives the whole
  tree, with a renderer per component type. Its `[style]` binding flattens an object into
  `setStyle` calls and refuses nested objects, and a binding on an element no directive matches
  reaches `setProperty` as it is when the template allows custom elements.
- Svelte 5 has no custom renderers: its compiler writes DOM operations (client) or string
  concatenation (server). An API is in review (sveltejs/svelte#18042), as an experimental compile
  option.
- Template languages keep white space that JSX drops, and write bare attributes (`hyphenate`) as
  empty strings in Vue and Angular.

## Decision

**One core, four front ends.** `@inkoxide/tree` holds what the front ends share: the host types,
the list of components (`COMPONENTS`) and the types of the props (`@inkoxide/tree/props`), the
placement rules (`checkElement`, `checkText`, `checkTree`), host nodes, `buildDocument`, and what
every front end does around its render (`templateFunctions`, `templateError`, `finishTemplate`).
A node keeps an opaque `origin` and a `trace` that names the components above it; the builder asks
the front end to render a `render` prop for a page (`renderPage`, in `BuildHooks`).
`@inkoxide/render` takes a `FrontEnd` (how to build the tree of an input, of a template with its
data, and the name of its component) and `createRenderer` gives the functions each front end
exports; its browser entry gives the browser helpers their state (`PdfRun`). Each framework
package is one import: components, Node functions, a `./browser` entry and `./templates`, with the
same names (Angular's are `injectPage` and `injectPdf`, and directives in place of the browser
components; Svelte's `createPdf`). What a front end's users render is what the framework renders:
an element in React, a VNode or a component in Vue, a component with its props or inputs in Svelte
and Angular.

**The same document in every framework.** `trimTemplateText` joins the lines of template text as
JSX does and drops the white space between elements outside text runs; `BOOLEAN_PROPS` turns a
bare attribute into `true`; SVG attributes are written by name, so the markup does not depend on
the order a framework sets them in. The conformance suite (`packages/conformance`) writes the
examples in each framework: each must build React's document and PDF, byte for byte.

**Vue: a custom renderer.** The components pass their attributes to the host element (camelized,
`class` as `className`), their default slot as content, and a `render` slot as the per-page
prop, rendered in a root of its own that inherits what the components around it provide. Async
components, async `setup()`, `<Suspense>`, Vapor components (which render without VNodes) and
listeners are template errors. A server build compiles single-file components to strings;
`@inkoxide/vue/vite` compiles the templates for the client.

**Angular: a renderer, and elements.** Templates use the `ink-*` elements with
`CUSTOM_ELEMENTS_SCHEMA`, and the renderer reads attributes, property bindings, classes and
styles; `[inkStyle]` takes what `[style]` cannot. Component hosts are replaced by their content,
and `[render]` takes an `ng-template`; listeners are errors. The stack of an error a template
throws comes from its trace (`Invoice_Template`) and the renderers, which know each component's
host. The package ships no Angular components, so it needs no Angular compilation (no ng-packagr,
whose TypeScript 6 the repository does not use): its browser helper is `injectPdf`, signals, and
its directives (`inkPdfViewer`, `inkPdfDownload`, `inkBlobProvider`) are written as partial
compilation writes a library (`ɵɵngDeclareDirective`), which an application's build links.
Typed components can come later, beside the elements. Angular 21 and 22 are tested in CI.

**Svelte: read back what it rendered.** Each component renders an `ink-*` element whose
`data-ink` is its place in a table of the render (given by context), where it puts its props and
its `render` snippet. On a server, the HTML of `svelte/server` is parsed; in a browser, the DOM
that `mount` builds in an element of no document is read. A snippet renders for each page with
the contexts around its element. An `ink-*` element that no component rendered (markup, `{@html}`)
is an error. Svelte keeps no component stack: the stack of a thrown error comes from its trace,
where each component is the function of its file. The components are generated from
`@inkoxide/tree`'s list, with their types (`.svelte.d.ts`). Svelte 5.16 is the oldest version the
tests pass on (5.0's compiler refuses the components' TypeScript); CI tests it. When Svelte ships
custom renderers, a renderer can replace the reading without changing the components' API.

## Consequences

- 1.0.0 breaks one import: React templates import from `@inkoxide/react` instead of
  `@inkoxide/render`, which now renders documents for every front end ([UPGRADING.md](../../UPGRADING.md)).
  `@inkoxide/react` keeps the names it exported before.
- `timings.reactMs` is `timings.templateMs`: the time of any front end's render.
- An SVG drawing writes its attributes by name, so that its markup does not depend on the order a
  framework sets them in.
- Component stacks name the user's components in React and Vue; in Angular and Svelte, those of a
  placement error, and those of a thrown error where the code keeps its names (not in a minified
  bundle).
- Angular templates lose template type checking of the elements' props; Svelte templates render
  on a server only when compiled for it (SvelteKit, Vite's SSR).
- The Svelte components are generated (`scripts/gen-svelte-components.mjs`), and Vitest runs a
  DOM project for the tests that need Svelte's client runtime.
- `examples/frameworks` renders the invoice in Vue, Svelte and Angular in the browsers, with the
  Tailwind plugin; a server build of the Vue and Svelte templates (as Nuxt and SvelteKit build
  them) is tested in `packages/conformance`, in place of apps of those frameworks.
