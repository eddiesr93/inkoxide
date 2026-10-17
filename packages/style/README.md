# @inkoxide/style

Reads CSS values into Inkoxide styles: lengths (with `calc()`, `em`, `vw` and `vh`), colours,
`flex`, transforms, transform origins, object positions and OpenType features.
`@inkoxide/react` uses it for `style` props and `@inkoxide/tailwind` for classes, so both read
a value the same way.

To make PDFs, install [`@inkoxide/render`](https://www.npmjs.com/package/@inkoxide/render)
instead: it brings this package with it. Its
[guide](https://github.com/eddiesr93/inkoxide/blob/main/packages/render/GUIDE.md#styling)
describes the styles.

Part of [Inkoxide](https://github.com/eddiesr93/inkoxide). Requires Node.js 22 or later; ES
modules only.

```sh
npm install @inkoxide/style
```

## Usage

```js
import { parseColor, parseTransform, PT_PER_UNIT } from '@inkoxide/style';

parseColor('#f80'); // [1, 0.533333, 0, 1]: RGBA, each from 0 to 1
parseTransform('transform', 'rotate(10deg) translate(4px, 50%)'); // [{ rotate: 10 }, { translate: [3, '50%'] }]
PT_PER_UNIT.mm; // 2.834645669291339 (72 / 25.4)
```

Lengths come out in points (1/72 inch): `1px` is 0.75pt and `1rem` 12pt, as in
`@inkoxide/render`.

## Licence

MIT
