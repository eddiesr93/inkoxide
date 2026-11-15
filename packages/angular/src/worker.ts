// `@inkoxide/angular/worker`: the script of the engine's Web Worker, for a worker the application
// makes itself. The Angular CLI bundles the workers of an application's own code, not those of its
// packages, so the application has a worker file that imports this one, and gives it to `init`:
//
// ```ts
// // src/pdf.worker.ts
// import '@inkoxide/angular/worker';
//
// // src/main.ts
// init({ worker: () => new Worker(new URL('./pdf.worker', import.meta.url), { type: 'module' }), … });
// ```
import '@inkoxide/render/worker';
