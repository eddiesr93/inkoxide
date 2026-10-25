// An Angular CLI application whose template uses Tailwind classes: test/angular-cli.test.ts builds
// it, with the Tailwind plugin (the class table) and without (Tailwind's compiler).
import { bootstrapApplication } from '@angular/platform-browser';
import { initTailwind, tw } from '@inkoxide/tailwind';

import { Card } from './card';

await initTailwind();
const app = await bootstrapApplication(Card);
const card = app.components[0]?.location.nativeElement as Element | undefined;
console.log(tw(card?.querySelector('ink-view')?.getAttribute('class') ?? ''));
