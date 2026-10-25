import type { TemplateTrack, TrackSize, TransformOp } from '@inkoxide/schema';
import { expect, test } from 'vitest';

// Checked by `pnpm typecheck`: each `@ts-expect-error` must be an error.
test('a transform function or a track object takes the keys of one form only', () => {
  const ok: (TransformOp | TrackSize | TemplateTrack)[] = [
    { translate: [1, 2] },
    { rotate: 5 },
    { min: 0, max: '1fr' },
    { fitContent: 10 },
    { repeat: 2, tracks: [10] },
  ];
  // @ts-expect-error two transform functions in one object
  const transform: TransformOp = { translate: [1, 2], rotate: 5 };
  // @ts-expect-error `{min, max}` and `{fitContent}` together
  const track: TrackSize = { min: 0, max: '1fr', fitContent: 10 };
  // @ts-expect-error a track and a repeat together
  const template: TemplateTrack = { min: 0, max: '1fr', repeat: 2, tracks: [10] };
  expect([...ok, transform, track, template]).toHaveLength(8);
});
