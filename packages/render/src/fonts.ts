import type { FontFace } from '@inkoxide/react';

/**
 * The fonts registered at start-up, from the package's assets/fonts.
 * All are under the SIL Open Font License.
 */
export const DEFAULT_FACES: readonly FontFace[] = [
  { id: 'Inter-Regular', family: 'Inter', weight: 400, file: 'Inter-Regular.ttf' },
  { id: 'Inter-Italic', family: 'Inter', weight: 400, style: 'italic', file: 'Inter-Italic.ttf' },
  { id: 'Inter-Bold', family: 'Inter', weight: 700, file: 'Inter-Bold.ttf' },
  { id: 'InterDisplay-Bold', family: 'Inter Display', weight: 700, file: 'InterDisplay-Bold.otf' },
  { id: 'NotoSans-Regular', family: 'Noto Sans', weight: 400, file: 'NotoSans-Regular.ttf' },
  { id: 'RobotoMono-Regular', family: 'Roboto Mono', weight: 400, file: 'RobotoMono-Regular.ttf' },
];
