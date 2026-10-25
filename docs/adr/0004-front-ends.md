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
the list of components (`COMPONENTS`), the types of the props (`@inkoxide/tree/props`) and of each
component's props by its name (`PropsByComponent`, which React's and Vue's components and the
generated Svelte ones are typed from), the placement rules (`checkElement`, `checkText`,
`checkTree`), host nodes, `buildDocument`, and what every front end does around its render
(`templateFunctions`, `templateError`, and `finishTemplate`, which drops the comments a template
language leaves). `@inkoxide/tree/common` has the names every front end exports as they are
(`TemplateError`, `StyleSheet`, `facturX`, …), and `@inkoxide/tree/internal` what Inkoxide's own
renderers share (`assignProp`, `nodeLinks`, stacks from traces), outside semver, as
`@inkoxide/style/internal` is. A node keeps an opaque `origin` and a `trace` that names the
components above it; the builder asks the front end to render a `render` prop for a page
(`renderPage`, which `buildDocument` takes), in a root of its own. `@inkoxide/render` takes a `FrontEnd` (how to build the tree
of an input, of a template with its data, and the name of its component) and `createRenderer`
gives the functions each front end exports, documented once on `NodeRenderer` and
`BrowserRenderer`; `@inkoxide/render/templates` keeps what templates served by id share in every
front end. Its browser entry gives the browser helpers their state (`PdfRun`), and
`sameDocument` (`viewerUrl` gives a frame its URL): a document made again with the same props (an element of the same type, key and
props; a component with the same props or inputs) does not render again. Each framework package
is one import: components, Node functions, a `./browser` entry and `./templates`, with the same
names (`usePage` in React, Vue and Svelte; Angular's are `injectPage` and `injectPdf`, as its own
`inject` functions are named, and directives in place of the browser components; Svelte's
`createPdf`). What a front end's users render is what the framework renders: an element in React,
a VNode or a component in Vue, a component with its props or inputs in Svelte and Angular. Only
`@inkoxide/react` reports its version, to React's DevTools through the reconciler; the other
frameworks have no such place.

**What a render leaves.** A container may keep `dispose`, which releases what the front end
rendered into it: Vue unmounts its app, Angular destroys its component or the view of a `render`
template, React unmounts its root, and Svelte, which reads what it rendered and unmounts at once,
needs none. The document keeps its container and those of its pages' `render` roots, of every
rebuild, and `TreeResult.dispose()` releases them all: the render functions call it once the PDF
is made (or the render failed), and a render the placement check refuses is released at once.
Until then the nodes keep their component stacks, which a front end may read from what it
rendered.

**The same document in every framework.** `trimTemplateText` joins the lines of template text as
JSX does and drops the white space between elements outside text runs; `BOOLEAN_PROPS` turns a
bare attribute into `true`; SVG attributes are written by name, so the markup does not depend on
the order a framework sets them in. The conformance suite (`packages/conformance`) writes the
examples in each framework: each must build React's document and PDF, byte for byte, and a
component in the wrong place, inside a component of the template, must give React's component
stack.

**Vue: a custom renderer.** The components pass their attributes to the host element (camelized,
`class` as `className`), their default slot as content, and a `render` slot as the per-page
prop, rendered in a root of its own that inherits what the components around it provide. Async
components, async `setup()`, `<Suspense>`, Vapor components (which render without VNodes; Vue 3.6
mounts them through the app's context, which the renderer fills with an error) and listeners are
template errors. A server build compiles single-file components to strings;
`@inkoxide/vue/vite` compiles the PDF templates for the client in Vite's builds (Vite's SSR, a
Nuxt app's server bundle). Nuxt's server routes are Nitro's, which compiles no `.vue` files: their
templates are render functions.

**Angular: a renderer, and elements.** Templates use the `ink-*` elements with
`CUSTOM_ELEMENTS_SCHEMA`, and the renderer reads attributes, property bindings, classes and
styles; `[inkStyle]` takes what `[style]` cannot (`@media` blocks, lists of styles). Component
hosts are replaced by their content, and `[render]` takes an `ng-template`; listeners are errors.
A node's stack is React's: the renderer links each node to its parent, and the stack names the
elements around it and, for a component's host element, the component; a `render` template's
content goes on with the element whose template it is. The stack of an error a template throws
names the components in its trace (`Invoice_Template`), then the elements around the innermost
one's host. The package ships no Angular components, so it needs no Angular compilation of its
own: its browser helper is `injectPdf`, signals, and its directives (`inkPdfViewer`,
`inkPdfDownload`, `inkBlobProvider`) are written as partial compilation writes a library
(`ɵɵngDeclareDirective`), with the types it writes in a library's declaration files, which an
application's build reads; the build links them. Angular 21 and 22 are tested in CI, ahead of
time too: `packages/conformance/test/angular-aot.test.ts` compiles an application with `ngc`.

**Svelte: read back what it rendered.** Each component renders an `ink-*` element whose
`data-ink` is its place in a table of the render (given by context), where it puts its props, its
`render` snippet, and an `Error` made where it was created, whose trace names the components of
the template around it. On a server, the HTML of `svelte/server` is parsed; in a browser, the DOM
that `mount` builds in an element of no document is read, then unmounted. A snippet renders for
each page with the contexts around its element. An `ink-*` element that no component rendered
(markup, `{@html}`) is an error. Svelte keeps no component stack: stacks come from traces, where
each component is the function of its file. The components are generated from `@inkoxide/tree`'s
list, with their types (`.svelte.d.ts`). Svelte 5.39 is the oldest version the package supports:
before it, a server render that threw left its contexts to the next render (fixed in 5.39.0), and
5.0's compiler refuses the components' TypeScript; CI tests 5.39. When Svelte ships custom
renderers, a renderer can replace the reading without changing the components' API.

## Consequences

- 1.0.0 breaks one import: React templates import from `@inkoxide/react` instead of
  `@inkoxide/render`, which now renders documents for every front end ([UPGRADING.md](../../UPGRADING.md)).
  `@inkoxide/react` keeps the names it exported before.
- `timings.templateMs` is the time of any front end's render; `timings.reactMs`, its name before,
  stays as a deprecated alias.
- An SVG drawing writes its attributes by name, so that its markup does not depend on the order a
  framework sets them in.
- Component stacks are React's in every front end. In Svelte, and for an error an Angular template
  throws, they come from traces, which name the components where the code keeps its names (not
  in a minified bundle).
- Angular templates lose template type checking of the elements' props. An Angular CLI build
  takes the Tailwind plugin through `@angular-builders/custom-esbuild` (the Angular guide). Svelte
  templates render on a server only when compiled for it (SvelteKit, Vite's SSR).
- The Svelte components are generated (`scripts/gen-svelte-components.mjs`), and Vitest runs a
  DOM project for the tests that need Svelte's client runtime.
- `examples/frameworks` renders the invoice in Vue, Svelte and Angular in the browsers, with the
  Tailwind plugin, as `examples/browser` renders React's: under a Content Security Policy,
  cross-origin isolated on a pool of threads, and with its scripts from another origin. A server
  build of the Vue and Svelte templates (as Vite's SSR builds them) is tested in
  `packages/conformance`, in place of applications of those frameworks.

## Alternatives

- **Typed Angular components** (`<ink-view>` as components with `input()`, compiled with
  ng-packagr): template type checking of the props, at the cost of an Angular compilation of the
  package. ng-packagr and Angular's compiler (21 and 22) need TypeScript 6.0, beside the
  repository's TypeScript 7, and the package's build would be unlike every other's. The elements
  need none of it; typed components can come later, beside them. Only the ahead-of-time test
  installs TypeScript 6 and `@angular/compiler-cli`, in `packages/conformance`.
- **A Svelte renderer** (sveltejs/svelte#18042): still in review, and experimental; reading back
  what Svelte rendered works with the Svelte 5 releases there are.
- **One input for every front end** (`(Component, props)`): each framework's own value instead
  (an element, a VNode, a component with its props or inputs), which its users already write.
- **A spike per framework first**: the conformance suite was the criterion instead, from the
  start.
- **Nuxt and SvelteKit applications in examples**: a Vite server build of the templates, in the
  conformance suite, tests what they do on a server, without two more applications to keep.
- **Leaving a render's app mounted**: less code, but a template that reads a store or a global
  signal would stay subscribed to it, once for every render.
- **A version reported by every front end**: only React has a place for it (DevTools).
