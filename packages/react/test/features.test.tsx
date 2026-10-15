import type { BoxNode, FieldNode, ImageNode, NoteNode, TableNode, TextNode } from '@inkoxide/schema';
import { parseColor } from '@inkoxide/tailwind';
import type { ReactNode } from 'react';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  Canvas,
  type CanvasGradient,
  Cell,
  Checkbox,
  Circle,
  ClipPath,
  type ClipPathProps,
  Defs,
  type DefsProps,
  Document,
  Field,
  FieldSet,
  Footer,
  G,
  Header,
  Image,
  ImageBackground,
  type ImageSrc,
  imageSource,
  isImagePath,
  LinearGradient,
  Link,
  List,
  Marker,
  type MarkerProps,
  Note,
  PAGE_SIZES,
  Page,
  PageBreak,
  PageNumber,
  type Painter,
  Path,
  Polyline,
  RadialGradient,
  Radio,
  Rect,
  remoteAllowed,
  renderTemplate,
  Row,
  Select,
  Spacer,
  Stop,
  type StyleProp,
  Svg,
  Table,
  Text,
  TextField,
  TextInput,
  type TextFormat,
  TotalPages,
  Tspan,
  type TspanProps,
  usePage,
  View,
} from '../src/index.ts';
import { body, first, registry, setup, thrown } from './helpers.ts';

beforeAll(setup);

const fonts = registry();
const tree = (children: ReactNode, opts: { pageCount?: number; page?: Record<string, unknown> } = {}) =>
  renderTemplate(
    <Document lang="en-US">
      <Page className="text-[10pt]" {...opts.page}>
        {children}
      </Page>
    </Document>,
    {
      fonts,
      pageCount: opts.pageCount,
      remoteImages: { allow: ['cdn.example.com', 'https://img.example.com/public/'] },
    },
  );
const decode = (data: string | undefined): string =>
  Buffer.from((data ?? '').replace(/^data:image\/svg\+xml;base64,/, ''), 'base64').toString('utf8');

describe('paint styles', () => {
  it('composes translate, rotate, scale and transform in CSS order', () => {
    const r = tree(<View className="translate-x-2 rotate-45 scale-110 skew-x-6 origin-top-left z-10" />);
    expect(first<BoxNode>(r).style).toEqual({
      transformOrigin: [0, 0],
      zIndex: 10,
      transform: [{ translate: [6, 0] }, { rotate: 45 }, { scale: [1.1, 1.1] }, { skew: [6, 0] }],
    });
  });

  it('keeps a lone rotation as rotate', () => {
    expect(first<BoxNode>(tree(<View className="rotate-12" />)).style).toEqual({ rotate: 12 });
  });

  it('maps per-corner radii and per-side border styles', () => {
    const r = tree(
      <>
        <View className="rounded-lg rounded-tl-none" />
        <View style={{ borderStyle: { top: 'dashed' }, borderRadius: { bottomRight: '2mm' } }} />
        <View className="border border-dashed [border-left-style:dotted]" />
      </>,
    );
    const [a, b, c] = body(r) as BoxNode[];
    expect(a?.style?.borderRadius).toEqual({ topLeft: 0, topRight: 6, bottomRight: 6, bottomLeft: 6 });
    expect(b?.style).toEqual({
      borderStyle: { top: 'dashed', right: 'solid', bottom: 'solid', left: 'solid' },
      borderRadius: { topLeft: 0, topRight: 0, bottomRight: 5.669291, bottomLeft: 0 },
    });
    expect(c?.style?.borderStyle).toEqual({
      top: 'dashed',
      right: 'dashed',
      bottom: 'dashed',
      left: 'dotted',
    });
  });

  it('lets a later border style or radius for every side replace earlier per-side values', () => {
    const r = tree(
      <>
        <View style={[{ borderTop: '2px solid blue' }, { border: '1px dashed red' }]} />
        <View style={[{ borderWidth: 1, borderStyle: { top: 'dotted' } }, { borderStyle: 'dashed' }]} />
        <View style={[{ borderRadius: { topLeft: 4 } }, { borderRadius: 8 }]} />
        <View className="rounded-tl-lg" style={{ borderRadius: 8 }} />
        <View
          className="border-dotted [border-top-style:dashed]"
          style={{ borderWidth: 1, borderStyle: 'solid' }}
        />
      </>,
    );
    const [a, b, c, d, e] = (body(r) as BoxNode[]).map((n) => n.style);
    expect(a?.borderStyle).toBe('dashed');
    expect(a?.borderColor?.top).toEqual(parseColor('red'));
    expect(b?.borderStyle).toBe('dashed');
    expect(c?.borderRadius).toBe(8);
    expect(d?.borderRadius).toBe(8);
    expect(e?.borderStyle).toBeUndefined();
  });

  it('keeps the corners a per-corner radius does not name', () => {
    const r = tree(
      <>
        <View className="rounded-lg" style={{ borderRadius: { topLeft: 0 } }} />
        <View style={[{ borderRadius: 6 }, { borderRadius: { topLeft: 0 } }]} />
        <View className="px-4" style={{ padding: { top: 5 } }} />
      </>,
    );
    const [a, b, c] = (body(r) as BoxNode[]).map((n) => n.style);
    const corners = { topLeft: 0, topRight: 6, bottomRight: 6, bottomLeft: 6 };
    expect(a?.borderRadius).toEqual(corners);
    expect(b?.borderRadius).toEqual(corners);
    expect(c?.padding).toEqual({ top: 5, right: 12, left: 12 });
  });

  it('accepts lengths with units and CSS transform strings in the style prop', () => {
    const r = tree(
      <View
        style={{
          width: '50mm',
          padding: { top: '1cm' },
          transform: 'rotate(10deg) translateY(2pt)',
          transformOrigin: 'bottom right',
        }}
      />,
    );
    expect(first<BoxNode>(r).style).toEqual({
      width: 141.732283,
      padding: { top: 28.346457 },
      transform: [{ rotate: 10 }, { translate: [0, 2] }],
      transformOrigin: ['100%', '100%'],
    });
    expect(thrown(() => tree(<View style={{ width: '3ex' as never }} />)).message).toContain(
      'width: 3ex (unsupported unit "ex")',
    );
    // Keywords in either order; names of Object.prototype are not keywords.
    const centerLeft = tree(<View style={{ transformOrigin: 'center left', rotate: 10 }} />);
    expect(first<BoxNode>(centerLeft).style?.transformOrigin).toEqual([0, '50%']);
    expect(
      thrown(() => tree(<View style={{ transformOrigin: 'left toString', rotate: 10 }} />)).message,
    ).toContain('transformOrigin: toString');
    expect(
      thrown(() => tree(<Text style={{ fontWeight: 'constructor' as never }}>x</Text>)).message,
    ).toContain('CSS weight name');
    // Colour keywords are case-insensitive.
    const clear = tree(<View style={{ backgroundColor: 'Transparent' }} />);
    expect(first<BoxNode>(clear).style?.backgroundColor).toEqual([0, 0, 0, 0]);
  });

  it('accepts the wrap, break and minPresenceAhead aliases and weight names', () => {
    const r = tree(
      <>
        <View {...{ wrap: false, minPresenceAhead: 40 }}>
          <Text>a</Text>
        </View>
        <View break>
          <Text style={{ fontWeight: 'bold' }}>b</Text>
        </View>
      </>,
    );
    const [a, b] = (r.document.pages[0].children ?? []) as BoxNode[];
    expect(a?.break).toEqual({ inside: 'avoid', minPresenceAhead: 40 });
    expect(b?.break).toEqual({ before: 'page' });
    const bold = b?.children?.[0] as TextNode | undefined;
    expect(bold?.runs[0]).toMatchObject({ font: 'Inter-Bold' });
    expect(thrown(() => tree(<Text style={{ fontWeight: 'heavyish' as never }}>x</Text>)).message).toContain(
      'CSS weight name',
    );
  });

  it('reads flex and offsets, and rejects style keys it does not know', () => {
    const r = tree(
      <>
        <View style={{ flex: 1 }} />
        <View style={{ flex: '2 0 10mm', flexShrink: 3 }} />
        <View style={{ position: 'absolute', top: 0, right: '1cm' }} />
        <View style={{ flex: '1 auto' }} />
      </>,
    );
    const [a, b, c, d] = (r.document.pages[0].children ?? []) as BoxNode[];
    expect(a?.style).toEqual({ flexGrow: 1, flexShrink: 1, flexBasis: '0%' });
    expect(b?.style).toEqual({ flexGrow: 2, flexShrink: 3, flexBasis: 28.346457 });
    expect(c?.style).toEqual({ position: 'absolute', inset: { top: 0, right: 28.346457 } });
    // `<grow> <basis>`, as in Tailwind's `flex-[1_auto]`.
    expect(d?.style).toEqual({ flexGrow: 1, flexShrink: 1, flexBasis: 'auto' });
    // A lone basis is `1 1 <basis>`, as in CSS, in a style and a class.
    const basis = { flexGrow: 1, flexShrink: 1, flexBasis: '30%' };
    expect(first<BoxNode>(tree(<View style={{ flex: '30%' }} />)).style).toEqual(basis);
    expect(first<BoxNode>(tree(<View className="flex-[30%]" />)).style).toEqual(basis);
    // Grow and shrink are never negative, in a style or a class.
    expect(thrown(() => tree(<View style={{ flex: '-1' }} />)).message).toContain('cannot be negative');
    const numeric = thrown(() => tree(<View style={{ flex: -1 }} />));
    expect([numeric.kind, numeric.message]).toEqual([
      'props',
      'Invalid style prop on <View>: flex -1: grow and shrink cannot be negative',
    ]);
    expect(thrown(() => tree(<View className="flex-[-1]" />)).message).toContain('flex-[-1]');
    for (const key of ['flexGrow', 'flexShrink'] as const) {
      expect(thrown(() => tree(<View style={{ [key]: -1 }} />)).message).toBe(
        `Invalid style prop on <View>: ${key} -1: cannot be negative`,
      );
      expect(thrown(() => tree(<View style={{ [key]: '30%' as never }} />)).message).toBe(
        `Invalid style prop on <View>: ${key} "30%" is not a number`,
      );
    }
    expect(first<BoxNode>(tree(<View style={{ flexGrow: 0, flexShrink: 2 }} />)).style).toEqual({
      flexGrow: 0,
      flexShrink: 2,
    });
    expect(thrown(() => tree(<View style={{ boxShadow: '0 1px 2px black' } as never} />)).message).toContain(
      'unknown style property "boxShadow"',
    );
  });
});

describe('relative units in the style prop', () => {
  it('converts pc and rem (Tailwind’s 12pt root size)', () => {
    const r = tree(<View style={{ width: '2pc', height: '1.5rem', borderTop: '0.1rem solid' }} />);
    expect(first<BoxNode>(r).style).toMatchObject({ width: 24, height: 18, borderWidth: { top: 1.2 } });
  });

  it('resolves em against the element’s font size, and fontSize against the parent’s', () => {
    const r = tree(
      <>
        <View style={{ padding: '1em' }} />
        <View className="text-[20pt]" style={{ marginTop: '0.5em' }} />
        <Text style={{ fontSize: '2em', paddingLeft: '1em', letterSpacing: '0.1em' }}>a</Text>
        <View style={[{ paddingTop: '1em' }, { fontSize: 15 }]} />
        <View style={{ fontSize: '1.5em' }}>
          <Text style={{ fontSize: '2em' }}>
            b <Text style={{ fontSize: '0.5em' }}>c</Text>
          </Text>
        </View>
      </>,
    );
    const [a, b, c, d, e] = body(r) as BoxNode[];
    expect(a?.style?.padding).toEqual({ top: 10, right: 10, bottom: 10, left: 10 });
    expect(b?.style?.margin).toEqual({ top: 10 });
    const text = c as unknown as TextNode;
    expect(text.style?.padding).toEqual({ left: 20 });
    expect(text.runs[0]).toMatchObject({ size: 20, letterSpacing: 2 });
    expect(d?.style?.padding).toEqual({ top: 15 });
    const runs = ((e?.children ?? []) as TextNode[])[0]?.runs;
    expect(runs?.map((x) => x.size)).toEqual([30, 15]);
  });

  it('resolves vw and vh against the page size', () => {
    const r = tree(<View style={{ width: '50vw', height: '10vh' }} />, {
      page: { size: 'A4-landscape', margin: { top: '5vh', left: '1em' } },
    });
    expect(first<BoxNode>(r).style).toEqual({ width: 420.945, height: 59.528 });
    expect(r.document.pages[0].margin).toEqual({ top: 29.764, left: 10 });
    expect(thrown(() => tree(null, { page: { size: { width: '50vw', height: 100 } } })).message).toContain(
      'size: 50vw (depends on the font or page size',
    );
  });

  it('rejects lengths and style numbers that are not finite, which JSON would drop', () => {
    const huge = '9'.repeat(400);
    for (const unit of ['mm', 'vw', 'em'] as const) {
      const width = `${huge}${unit}` as `${number}${typeof unit}`;
      expect(thrown(() => tree(<View style={{ width }} />)).message).toContain(
        `width: ${huge}${unit} (too large a number)`,
      );
    }
    expect(thrown(() => tree(<View style={{ width: 100 / 0 }} />)).message).toBe(
      'Invalid style prop on <View>: width: Infinity is not a finite number',
    );
    expect(thrown(() => tree(<View style={{ marginTop: Number.NaN }} />)).message).toContain(
      'marginTop: NaN is not a finite number',
    );
    expect(thrown(() => tree(<View style={{ transform: `rotate(${huge}deg)` }} />)).message).toContain(
      `transform: ${huge}deg (too large a number)`,
    );
    expect(thrown(() => tree(<View style={{ transform: [{ rotate: 1 / 0 }] }} />)).message).toContain(
      'transform: Infinity is not a finite number',
    );
  });

  it('reports the bound a length is past, not a oneOf branch that does not apply', () => {
    const problems = (style: Record<string, unknown>) =>
      tree(<View style={style} />)
        .validate()
        .map((p) => `${p.path}: ${p.message}`);
    expect(problems({ width: '20000%' })[0]).toBe(
      'pages[0].children[0].style.width: must be a percentage within ±10000%, like "50%"',
    );
    expect(problems({ width: 2e6 })[0]).toBe('pages[0].children[0].style.width: must be <= 1000000');
    expect(problems({ transform: 'scale(2000000)' })[0]).toBe(
      'pages[0].children[0].style.transform[0].scale[0]: must be <= 1000000',
    );
    expect(problems({ width: '100%', transform: 'translate(-50%, 1pt)' })).toEqual([]);
  });

  it('reports the pattern of the unit a track was written in', () => {
    const firstProblem = (style: Record<string, unknown>) => {
      const [p] = tree(<View style={{ display: 'grid', ...style }} />).validate();
      return p && `${p.path}: ${p.message}`;
    };
    expect(firstProblem({ gridTemplateColumns: ['1000001fr'] })).toBe(
      'pages[0].children[0].style.gridTemplateColumns[0]: must be a flexible size, a share of the free space, of at most 1000000fr, like "1fr"',
    );
    expect(firstProblem({ gridTemplateColumns: ['20000%'] })).toBe(
      'pages[0].children[0].style.gridTemplateColumns[0]: must be a percentage within ±10000%, like "50%"',
    );
  });
});

describe('lengths in style and className', () => {
  /** The style of a View with the length `v` as its width, from `style` and from `w-[…]`. */
  const widths = (v: string, own: { style?: StyleProp; className?: string } = {}) => {
    const style = { ...own.style, width: v } as StyleProp;
    const className = `${own.className ?? ''} w-[${v.replaceAll(' ', '_')}]`;
    return [
      first<BoxNode>(tree(<View className={own.className} style={style} />)).style,
      first<BoxNode>(tree(<View className={className} style={own.style} />)).style,
    ];
  };

  it('read the same strings alike', () => {
    const lengths = {
      '5PX': 3.75,
      '1e1pt': 10,
      'calc(10pt + 2pt)': 12,
      '5 pt': 5,
      '2em': 20,
      '10vw': 59.528,
      '50%': '50%',
    };
    for (const [v, width] of Object.entries(lengths)) {
      const [fromStyle, fromClass] = widths(v);
      expect(fromStyle, v).toEqual({ width });
      expect(fromClass, v).toEqual({ width });
    }
    for (const bad of ['3ex', 'calc(100% - 10pt)']) {
      const fromStyle = thrown(() => widths(bad)[0]).message;
      const fromClass = thrown(() => tree(<View className={`w-[${bad.replaceAll(' ', '_')}]`} />)).message;
      expect(fromStyle, bad).toContain(`width: ${bad} (`);
      expect(fromClass, bad).toContain(`width: ${bad} (`);
    }
  });

  it('take em of the font size, the inherited one in fontSize, and the style’s over the class’s', () => {
    for (const [own, width] of [
      [{ style: { fontSize: '2em' } }, 40],
      [{ className: 'text-[2em]' }, 40],
      [{ className: 'text-[15pt]', style: { fontSize: '1.2em' } }, 24],
    ] as const) {
      expect(widths('2em', own), JSON.stringify(own)).toEqual([
        expect.objectContaining({ width }),
        expect.objectContaining({ width }),
      ]);
    }
    const sizes = (body(tree(<Text style={{ fontSize: '2em' }}>a</Text>)) as TextNode[]).concat(
      body(tree(<Text className="text-[2em]">a</Text>)) as TextNode[],
    );
    expect(sizes.map((t) => t.runs[0]?.size)).toEqual([20, 20]);
  });

  it('take em, vw and vh in a transform string', () => {
    const r = tree(<View style={{ transform: 'translate(1em, 10vh)', transformOrigin: '1em 0' }} />);
    expect(first<BoxNode>(r).style).toEqual({
      transform: [{ translate: [10, 84.189] }],
      transformOrigin: [10, 0],
    });
  });
});

describe('CSS names and shorthands in the style prop', () => {
  const black = [0, 0, 0, 1];
  const red = parseColor('red');

  it('reads border shorthand strings; sides win over border, and longhands over both', () => {
    const r = tree(
      <>
        <View style={{ border: '1px solid #e5e7eb' }} />
        <View
          style={{
            border: '1pt dotted',
            borderTop: '2pt dashed red',
            borderBottom: 'thin solid rgb(0 0 255)',
          }}
        />
        <View style={{ border: '1px solid red', borderBottomWidth: 3, borderStyle: 'dashed' }} />
        <View className="border border-dashed" style={{ border: 'none' }} />
      </>,
    );
    const [a, b, c, d] = body(r) as BoxNode[];
    const all = <T,>(v: T) => ({ top: v, right: v, bottom: v, left: v });
    expect(a?.style).toEqual({ borderWidth: all(0.75), borderColor: all(parseColor('#e5e7eb')) });
    expect(b?.style).toEqual({
      borderWidth: { top: 2, right: 1, bottom: 0.75, left: 1 },
      borderColor: { top: red, right: black, bottom: [0, 0, 1, 1], left: black },
      borderStyle: { top: 'dashed', right: 'dotted', bottom: 'solid', left: 'dotted' },
    });
    expect(c?.style).toEqual({
      borderWidth: { top: 0.75, right: 0.75, bottom: 3, left: 0.75 },
      borderColor: all(red),
      borderStyle: 'dashed',
    });
    expect(d?.style?.borderWidth).toEqual(all(0));
    for (const bad of ['1px wavy red', '1px 2px solid', 'solid dashed', '10% solid']) {
      expect(thrown(() => tree(<View style={{ borderTop: bad }} />)).message).toContain(
        `borderTop "${bad}" is not a border`,
      );
    }
  });

  it('reads line-through, both lines, and the textDecoration* names', () => {
    const r = tree(
      <>
        <Text
          style={{
            textDecoration: 'line-through',
            textDecorationColor: 'red',
            textDecorationStyle: 'dotted',
            textDecorationThickness: '1pt',
          }}
        >
          a
        </Text>
        <Text style={{ textDecoration: 'underline line-through' }}>b</Text>
        <Text className="underline" style={{ textDecoration: 'none' }}>
          c
        </Text>
      </>,
    );
    const [a, b, c] = (body(r) as TextNode[]).map((t) => t.runs[0]);
    expect(a).toMatchObject({
      decoration: 'lineThrough',
      decorationColor: red,
      decorationStyle: 'dotted',
      decorationThickness: 1,
    });
    expect(b?.decoration).toBe('lineThrough');
    expect(c).not.toHaveProperty('decoration');
    expect(
      thrown(() => tree(<Text style={{ textDecoration: 'overline' as never }}>x</Text>)).message,
    ).toContain('textDecoration: overline (supported: none, underline, line-through, or both lines)');
  });

  it('maps textOverflow to ellipsis', () => {
    const r = tree(
      <>
        <Text style={{ maxLines: 2, textOverflow: 'ellipsis' }}>a</Text>
        <Text className="truncate" style={{ textOverflow: 'clip' }}>
          b
        </Text>
      </>,
    );
    const [a, b] = body(r) as TextNode[];
    expect(a?.style).toEqual({ maxLines: 2, ellipsis: true });
    expect(b?.style).toEqual({ maxLines: 1 });
    expect(thrown(() => tree(<Text style={{ textOverflow: 'fade' as never }}>x</Text>)).message).toContain(
      'textOverflow "fade" is not ellipsis or clip',
    );
  });

  it('maps objectFit to the image fit; the fit prop wins', () => {
    const r = tree(
      <>
        <Image src="logo" style={{ objectFit: 'cover' }} />
        <Image src="logo" fit="contain" style={{ objectFit: 'cover' }} />
      </>,
    );
    const [a, b] = body(r) as ImageNode[];
    expect(a?.fit).toBe('cover');
    expect(b?.fit).toBe('contain');
    expect(thrown(() => tree(<View style={{ objectFit: 'cover' }} />)).message).toContain(
      'objectFit applies to <Image> and <ImageBackground>, not <View>',
    );
    expect(
      thrown(() => tree(<Image src="logo" style={{ objectFit: 'scale-down' as never }} />)).message,
    ).toContain('objectFit: scale-down (supported: fill, contain, cover, none)');
  });

  it('maps objectPosition and object-* classes to the image objectPosition', () => {
    const r = tree(
      <>
        <Image src="logo" fit="cover" style={{ objectPosition: 'left top' }} />
        <Image src="logo" fit="contain" style={{ objectPosition: 'bottom' }} />
        <Image src="logo" fit="none" style={{ objectPosition: '25% 75%' }} />
        <Image src="logo" fit="none" style={{ objectPosition: '10pt 20px' }} />
        <Image src="logo" fit="none" style={{ objectPosition: 'top right' }} />
        <Image src="logo" style={{ objectPosition: '5mm' }} />
        <Image src="logo" className="object-cover object-right-bottom" />
        <Image src="logo" className="object-left" style={{ objectPosition: 'center top' }} />
        <Image src="logo" fit="contain" />
        <Image src="logo" style={{ fontSize: 10, objectPosition: '2em 0' }} />
      </>,
    );
    const nodes = body(r) as ImageNode[];
    expect(nodes.map((n) => n.objectPosition)).toEqual([
      { x: 'left', y: 'top' },
      { x: 'center', y: 'bottom' },
      { x: '25%', y: '75%' },
      { x: 10, y: 15 },
      { x: 'right', y: 'top' },
      { x: 14.173228, y: 'center' },
      { x: 'right', y: 'bottom' },
      { x: 'center', y: 'top' },
      undefined,
      { x: 20, y: 0 },
    ]);
    expect(nodes[6]?.fit).toBe('cover');
    // Not a style property of the box.
    expect(nodes.every((n) => n.style === undefined || !('objectPosition' in n.style))).toBe(true);
  });

  it('throws on objectPosition off an Image or in a form the engine cannot place', () => {
    expect(thrown(() => tree(<View style={{ objectPosition: 'left' }} />)).message).toContain(
      'objectPosition applies to <Image> and <ImageBackground>, not <View>',
    );
    expect(thrown(() => tree(<Text className="object-top">x</Text>)).message).toContain(
      'objectPosition applies to <Image> and <ImageBackground>, not <Text>',
    );
    for (const bad of ['top 25%', '25% left', 'left right', 'right 10px top', 'middle', '10', 'auto', '']) {
      expect(thrown(() => tree(<Image src="logo" style={{ objectPosition: bad }} />)).message, bad).toContain(
        `objectPosition: ${bad}`,
      );
    }
    expect(
      thrown(() => tree(<Image src="logo" style={{ objectPosition: ['left', 'top'] as never }} />)).message,
    ).toContain('objectPosition must be a CSS string');
  });
});

describe('ImageBackground', () => {
  it('is a box with a background image, placed by fit, objectFit and objectPosition', () => {
    const r = tree(
      <>
        <ImageBackground src="logo" className="rounded-lg border p-4" style={{ height: 80 }} href="#x">
          over the image
        </ImageBackground>
        <ImageBackground src="logo" fit="contain" className="object-cover object-left-top" />
        <ImageBackground src="logo" style={{ objectFit: 'none', objectPosition: '25% 75%' }}>
          <View />
        </ImageBackground>
        <ImageBackground src="data:image/png;base64,AAAA" />
        <ImageBackground src={new Uint8Array([1, 2, 3])} />
      </>,
    );
    const [a, b, c, d, e] = body(r) as BoxNode[];
    expect(a).toMatchObject({ type: 'box', backgroundImage: { src: 'logo' }, link: '#x' });
    expect(a?.style).toMatchObject({ height: 80, borderRadius: 6 });
    expect((a?.children?.[0] as TextNode | undefined)?.runs[0]?.text).toBe('over the image');
    // The fit prop wins over objectFit; both stay off the box style.
    expect(b?.backgroundImage).toEqual({
      src: 'logo',
      fit: 'contain',
      objectPosition: { x: 'left', y: 'top' },
    });
    expect(c?.backgroundImage).toEqual({ src: 'logo', fit: 'none', objectPosition: { x: '25%', y: '75%' } });
    expect(c?.children).toHaveLength(1);
    for (const n of [a, b, c]) expect(n?.style ?? {}).not.toHaveProperty('objectFit');
    expect(d?.backgroundImage).toEqual({ data: 'data:image/png;base64,AAAA' });
    expect(e?.backgroundImage?.src).toBeTypeOf('string');
    expect(r.images[e?.backgroundImage?.src as string]).toEqual(new Uint8Array([1, 2, 3]));
  });

  it('needs a src, and points the props of other libraries to src and style', () => {
    expect(thrown(() => tree(<ImageBackground {...({} as { src: string })} />)).message).toContain(
      '<ImageBackground> needs a src',
    );
    for (const key of ['source', 'imageStyle', 'srcSet']) {
      const props = { src: 'logo', [key]: key === 'imageStyle' ? { opacity: 0.5 } : 'logo' };
      expect(thrown(() => tree(<ImageBackground {...props} />)).message, key).toContain(
        `<ImageBackground> does not take ${key}: pass the image as src`,
      );
    }
  });

  it('goes where a View goes and takes what a View takes', () => {
    expect(
      thrown(() =>
        tree(
          <Text>
            <ImageBackground src="logo" />
          </Text>,
        ),
      ).message,
    ).toContain('<ImageBackground> cannot be inside <Text>');
    const r = tree(
      <Table columns={[100]}>
        <Row>
          <Cell>
            <ImageBackground src="logo" className="h-10" />
          </Cell>
        </Row>
      </Table>,
    );
    const cell = (first<TableNode>(r).rows[0]?.cells[0]?.children ?? [])[0] as BoxNode;
    expect(cell.backgroundImage).toEqual({ src: 'logo' });
  });
});

describe('text features', () => {
  it('puts decoration colour, thickness and style on decorated runs only', () => {
    const r = tree(
      <Text className="decoration-red-500 decoration-2 decoration-wavy">
        plain <Text className="underline">wavy</Text>
      </Text>,
    );
    const runs = first<TextNode>(r).runs;
    expect(runs[0]).not.toHaveProperty('decorationColor');
    expect(runs[1]).toMatchObject({
      text: 'wavy',
      decoration: 'underline',
      decorationThickness: 1.5,
      decorationStyle: 'wavy',
    });
    expect(runs[1]?.decorationColor?.[0]).toBeCloseTo(0.98, 2);
  });

  it('maps indent, white space, direction, alignment, orphans and widows', () => {
    const r = tree(
      <>
        <Text className="-indent-4 whitespace-pre-wrap text-end" dir="rtl" orphans={2} widows={3}>
          שלום
        </Text>
        <Text className="text-left">left</Text>
        <Text className="text-start">start</Text>
      </>,
    );
    const [a, b, c] = body(r) as TextNode[];
    expect(a?.style).toEqual({
      textAlign: 'end',
      direction: 'rtl',
      textIndent: -12,
      orphans: 2,
      widows: 3,
      whiteSpace: 'pre-wrap',
    });
    expect(b?.style).toBeUndefined();
    expect(c?.style).toBeUndefined();
  });
});

describe('page filters, sizes and per-page content', () => {
  /** The size of a page with the `size` prop `s` and the other page props `page`. */
  const size = (s: unknown, page: Record<string, unknown> = {}) =>
    tree(null, { page: { size: s, ...page } }).document.pages[0].size;

  it('passes pages through and sizes pages from presets, orientation and units', () => {
    const r = tree(<Text pages="first">first page only</Text>, {
      page: { size: 'A3', orientation: 'landscape', margin: '15mm' },
    });
    expect(first<TextNode>(r).pages).toBe('first');
    expect(r.document.pages[0].size).toEqual({ width: 1190.55, height: 841.89 });
    expect(r.document.pages[0].margin?.top).toBeCloseTo(42.52, 2);
    const custom = tree(null, { page: { size: { width: '210mm', height: '297mm' } } });
    expect(custom.document.pages[0].size.width).toBeCloseTo(595.28, 2);
  });

  it('has every ISO A, B and C size, RA, SRA, the US sizes, Folio and ID-1', () => {
    const names = [
      '4A0',
      '2A0',
      ...['A', 'B', 'C'].flatMap((s) => Array.from({ length: 11 }, (_, i) => `${s}${i}`)),
      ...['RA', 'SRA'].flatMap((s) => Array.from({ length: 5 }, (_, i) => `${s}${i}`)),
      'Letter',
      'Legal',
      'Tabloid',
      'Executive',
      'Folio',
      'ID1',
    ];
    expect(Object.keys(PAGE_SIZES).sort()).toEqual(names.sort());
    for (const { width, height } of Object.values(PAGE_SIZES)) expect(width).toBeLessThan(height);
    expect(size('4A0')).toEqual({ width: 4767.87, height: 6740.79 });
    expect(size('A10')).toEqual({ width: 73.7, height: 104.88 });
    expect(size('B0')).toEqual({ width: 2834.65, height: 4008.19 });
    expect(size('C4')).toEqual({ width: 649.13, height: 918.43 });
    expect(size('RA4')).toEqual({ width: 609.45, height: 864.57 });
    expect(size('SRA3')).toEqual({ width: 907.09, height: 1275.59 });
    expect(size('Folio')).toEqual({ width: 612, height: 936 });
    expect(size('ID1')).toEqual({ width: 153.01, height: 242.65 });
  });

  it('matches preset names in any case, with or without "-landscape"', () => {
    const letter = { width: 612, height: 792 };
    expect(size('letter')).toEqual(letter);
    expect(size('LETTER')).toEqual(letter);
    expect(size('a4-landscape')).toEqual({ width: 841.89, height: 595.28 });
    expect(size('SRA2-Landscape')).toEqual({ width: 1814.17, height: 1275.59 });
    expect(size('id1-landscape')).toEqual({ width: 242.65, height: 153.01 });
  });

  it('sizes pages from [width, height], [width] and a bare length', () => {
    expect(size([400, 600])).toEqual({ width: 400, height: 600 });
    expect(size(['100mm', '2in'])).toEqual({ width: 283.464567, height: 144 });
    expect(size([300])).toEqual({ width: 300, height: 300 });
    expect(size(['1in'])).toEqual({ width: 72, height: 72 });
    expect(size(250)).toEqual({ width: 250, height: 250 });
    expect(size('3in')).toEqual({ width: 216, height: 216 });
    expect(size([400, 600], { orientation: 'landscape' })).toEqual({ width: 600, height: 400 });
    // Each form is typed.
    const typed = renderTemplate(
      <Document>
        <Page size="letter" />
        <Page size="SRA2-landscape" />
        <Page size={['100mm', 200]} />
        <Page size={[300]} />
        <Page size="80mm" />
      </Document>,
      { fonts },
    );
    expect(typed.document.pages.map((p) => p.size.width)).toEqual([
      612, 1814.17, 283.464567, 300, 226.771654,
    ]);
  });

  it('rejects unknown page sizes', () => {
    for (const bad of ['A11', 'D4', 'A4-sideways', 'A4-landscape-x', '50%']) {
      expect(thrown(() => tree(null, { page: { size: bad } })).message).toContain(
        `Unknown page size "${bad}"`,
      );
    }
    expect(thrown(() => tree(null, { page: { size: [] } })).message).toContain('[width, height] or [width]');
    expect(thrown(() => tree(null, { page: { size: [1, 2, 3] } })).message).toContain('not 3 values');
    expect(thrown(() => tree(null, { page: { size: ['10'] } })).message).toContain(
      'size: 10 (expected a length',
    );
    // `infinity` is a length only inside calc(), in a page size and a style.
    expect(thrown(() => tree(null, { page: { size: 'infinity' } })).message).toContain(
      'Unknown page size "infinity"',
    );
    expect(thrown(() => tree(null, { page: { size: ['infinity'] } })).message).toContain(
      'size: infinity (infinity is only allowed inside calc())',
    );
    for (const width of ['infinity', '-infinity']) {
      expect(thrown(() => tree(<View style={{ width: width as never }} />)).message).toContain(
        `width: ${width} (infinity is only allowed inside calc())`,
      );
    }
    // A page has no font size or page to be relative to, and the types say so.
    expect(thrown(() => tree(null, { page: { size: '10em' } })).message).toContain(
      'Unknown page size "10em"',
    );
    for (const bad of [['10em', 5], [5, '50vh'], ['2vw'], { width: 100, height: '1em' }]) {
      expect(thrown(() => tree(null, { page: { size: bad } })).message).toContain(
        'depends on the font or page size',
      );
    }
    const types = (
      <Document>
        {/* @ts-expect-error: relative units are not page sizes */}
        <Page size="10em" />
        {/* @ts-expect-error: relative units are not page sizes */}
        <Page size={['10em', 5]} />
        {/* @ts-expect-error: relative units are not page sizes */}
        <Page size={['2vw']} />
      </Document>
    );
    expect(types).toBeDefined();
  });

  it('renders a render prop once per page and groups identical output', () => {
    const label = ({ pageNumber, totalPages }: { pageNumber: number; totalPages: number }) =>
      pageNumber === totalPages ? 'Last page' : 'Continued →';
    const doc = (
      <Footer>
        <Text className="mt-2 text-right" render={label} />
      </Footer>
    );
    const one = tree(doc);
    expect(one.dynamic).toBe(true);
    expect((((one.document.pages[0].footer as BoxNode).children ?? []) as TextNode[])[0]?.runs[0]?.text).toBe(
      'Last page',
    );

    const three = one.rebuild(3);
    const wrapper = (three.document.pages[0].footer as BoxNode).children?.[0] as BoxNode;
    expect(wrapper.style).toEqual({ display: 'grid', margin: { top: 6 } });
    const variants = wrapper.children as TextNode[];
    expect(variants.map((v) => [v.pages, v.runs[0]?.text, v.style])).toEqual([
      [[1, 2], 'Continued →', { textAlign: 'right', gridRow: { start: 1 }, gridColumn: { start: 1 } }],
      [[3], 'Last page', { textAlign: 'right', gridRow: { start: 1 }, gridColumn: { start: 1 } }],
    ]);
  });

  it('gives components in a render prop the page through usePage()', () => {
    function Badge() {
      const { pageNumber } = usePage();
      return <Text>{pageNumber % 2 === 0 ? 'even' : 'odd'}</Text>;
    }
    const r = tree(<View render={() => <Badge />} />, { pageCount: 2 });
    const variants = (first<BoxNode>(r).children ?? []) as BoxNode[];
    expect(variants.map((v) => [v.pages, ((v.children ?? []) as TextNode[])[0]?.runs[0]?.text])).toEqual([
      [[1], 'odd'],
      [[2], 'even'],
    ]);
    function Outside() {
      usePage();
      return null;
    }
    expect(thrown(() => tree(<Outside />)).message).toContain('only inside a render prop');
  });
});

describe('fixed', () => {
  const corner = { position: 'absolute', bottom: 0, right: 0 } as const;

  it('repeats View, Text, Image, Svg and Canvas on every page through break.fixed', () => {
    const r = tree(
      <>
        <View fixed style={corner} id="stamp" href="https://example.com" pages="notFirst" />
        <Text fixed className="absolute top-0 left-0" break={false}>
          Draft
        </Text>
        <Image fixed src="logo" className="absolute right-0 bottom-0 h-4 w-4" />
        <Svg fixed width={10} height={10} className="absolute top-0 right-0">
          <Rect width={10} height={10} fill="#f00" />
        </Svg>
        <Canvas fixed width={10} height={10} style={corner} paint={(p) => p.rect(0, 0, 5, 5).fill('#000')} />
        <Link fixed href="#top" style={corner}>
          <Text>Top</Text>
        </Link>
        <View fixed={false} />
      </>,
    );
    const nodes = body(r) as Array<BoxNode | TextNode | ImageNode>;
    expect(nodes.map((n) => n.break)).toEqual([
      { fixed: true },
      { fixed: true },
      { fixed: true },
      { fixed: true },
      { fixed: true },
      { fixed: true },
      undefined,
    ]);
    expect(nodes.slice(0, 6).every((n) => n.style?.position === 'absolute')).toBe(true);
    expect(nodes[0]).toMatchObject({ id: 'stamp', link: 'https://example.com', pages: 'notFirst' });
    expect(nodes[0]?.style).toMatchObject({ inset: { bottom: 0, right: 0 } });
  });

  it('shows each page its own render output', () => {
    const footer = (
      <Text
        fixed
        style={{ position: 'absolute', bottom: -24, left: 0, right: 0, textAlign: 'center' }}
        render={({ pageNumber, totalPages }) => `${pageNumber} / ${totalPages}`}
      />
    );
    const three = tree(footer).rebuild(3);
    const wrapper = body(three)[0] as BoxNode;
    // The wrapper is the fixed, positioned element; each variant is painted on its page.
    expect(wrapper.break).toEqual({ fixed: true });
    expect(wrapper.style).toMatchObject({
      display: 'grid',
      position: 'absolute',
      inset: { bottom: -24, left: 0, right: 0 },
    });
    const variants = (wrapper.children ?? []) as TextNode[];
    expect(variants.map((v) => [v.pages, v.runs[0]?.text, v.break])).toEqual([
      [[1], '1 / 3', undefined],
      [[2], '2 / 3', undefined],
      [[3], '3 / 3', undefined],
    ]);
    // Placeholders work too; the engine fills them in on every page.
    const numbers = tree(
      <Text fixed style={corner}>
        <PageNumber /> of <TotalPages />
      </Text>,
    );
    expect(first<TextNode>(numbers).runs.map((run) => run.placeholder ?? run.text)).toEqual([
      'pageNumber',
      ' of ',
      'totalPages',
    ]);
  });

  it('explains where fixed cannot be used', () => {
    const message = (children: ReactNode) => thrown(() => tree(children)).message;
    expect(message(<View fixed />)).toContain("<View fixed>: a fixed element needs position: 'absolute'");
    expect(message(<Text fixed>x</Text>)).toContain('use <Header> or <Footer>');
    expect(message(<View fixed className="relative" />)).toContain("needs position: 'absolute'");
    expect(
      message(
        <View>
          <View fixed style={corner} />
        </View>,
      ),
    ).toContain('only an element placed directly in a <Page> can be fixed');
    expect(message(<View render={() => <View fixed style={corner} />} />)).toContain(
      'only an element placed directly in a <Page>',
    );
    expect(
      message(
        <Header>
          <Text fixed style={corner}>
            x
          </Text>
        </Header>,
      ),
    ).toContain('<Text fixed>: a <Header> is already on every page');
    expect(
      message(
        <Footer>
          <View fixed style={corner} />
        </Footer>,
      ),
    ).toContain('a <Footer> is already on every page');
    expect(
      message(
        <Table columns={[100]}>
          <Row>
            <Cell>
              <View fixed style={corner} />
            </Cell>
          </Row>
        </Table>,
      ),
    ).toContain('an element in a <Table> cannot be fixed');
    expect(
      message(
        <Text>
          a <Text {...({ fixed: true } as object)}>b</Text>
        </Text>,
      ),
    ).toContain('an inline run inside a <Text> cannot be fixed');
    expect(message(<Field kind="text" name="n" style={corner} {...({ fixed: true } as object)} />)).toContain(
      'a form field cannot be fixed',
    );
    // Pagination controls do nothing on a fixed element, inside one, or inside a header or footer.
    expect(message(<View fixed style={corner} break />)).toContain(
      '<View fixed>: break, wrap={false} and minPresenceAhead do nothing on a fixed element',
    );
    expect(message(<View fixed style={corner} break="avoid" />)).toContain('<View fixed>: break');
    expect(message(<View fixed style={corner} wrap={false} />)).toContain('<View fixed>: break');
    expect(message(<Text fixed style={corner} minPresenceAhead={20} />)).toContain('<Text fixed>: break');
    expect(message(<View fixed style={corner} break={{ after: 'page' }} />)).toContain('<View fixed>: break');
    expect(
      message(
        <View fixed style={corner}>
          <View>
            <Text break="page-after">x</Text>
          </View>
        </View>,
      ),
    ).toContain('<Text>: break, wrap={false} and minPresenceAhead do nothing inside a fixed <View>');
    expect(message(<View fixed style={corner} render={() => <View wrap={false} />} />)).toContain(
      '<View>: break, wrap={false} and minPresenceAhead do nothing inside a fixed <View>',
    );
    expect(
      message(
        <View fixed style={corner}>
          <PageBreak />
        </View>,
      ),
    ).toContain('<PageBreak> does nothing inside a fixed <View>');
    expect(
      message(
        <Header>
          <View minPresenceAhead={10} />
        </Header>,
      ),
    ).toContain('<View>: break, wrap={false} and minPresenceAhead do nothing inside a <Header>');
    expect(
      message(
        <Footer>
          <PageBreak />
        </Footer>,
      ),
    ).toContain('<PageBreak> does nothing inside a <Footer>');
    // A header or footer is on every page already.
    expect(message(<Header {...({ fixed: true } as object)} />)).toContain(
      '<Header fixed>: a <Header> is already on every page. Remove `fixed`.',
    );
    expect(message(<Footer {...({ fixed: true } as object)} />)).toContain(
      '<Footer fixed>: a <Footer> is already',
    );
    // Defaults are not controls; outside a fixed element, nothing changes.
    expect(first<BoxNode>(tree(<View fixed style={corner} break={{ before: 'auto' }} />)).break).toEqual({
      before: 'auto',
      fixed: true,
    });
    expect(first<BoxNode>(tree(<View wrap={false} />)).break).toEqual({ inside: 'avoid' });
    // CSS `position: fixed` points to the prop.
    expect(message(<View className="fixed" />)).toContain('give it the fixed prop and position: absolute');
    expect(message(<View style={{ position: 'fixed' as never }} />)).toContain('give it the fixed prop');
    const e = thrown(() => tree(<View fixed />));
    expect(e.kind).toBe('props');
    expect(e.componentStack).toContain('in View');
  });
});

describe('form fields', () => {
  it('builds fields with the inherited font, size and colour', () => {
    const r = tree(
      <View className="text-[11pt] text-blue-700">
        <TextField name="last-name" value="Johnson" maxLength={40} className="w-40" />
        <Checkbox name="consent" checked />
        <Radio name="type" value="individual" checked pages="first" />
        <Select name="state" options={['Ohio', 'Iowa']} value="Iowa" required />
        <Field kind="text" name="notes" multiline readOnly />
      </View>,
    );
    const [t, c, rd, s, m] = (first<BoxNode>(r).children ?? []) as FieldNode[];
    expect(t?.field).toMatchObject({
      name: 'last-name',
      kind: 'text',
      value: 'Johnson',
      maxLength: 40,
      font: 'Inter-Regular',
      size: 11,
    });
    expect(t?.style).toEqual({ width: 120 });
    expect(c?.field).toMatchObject({ kind: 'checkbox', checked: true });
    expect(c?.field).not.toHaveProperty('font');
    expect(rd).toMatchObject({ pages: 'first', field: { kind: 'radio', value: 'individual' } });
    expect(s?.field).toMatchObject({ kind: 'select', options: ['Ohio', 'Iowa'], required: true });
    expect(m?.field).toMatchObject({ multiline: true, readOnly: true });
    expect(r.validate()).toEqual([]);
  });

  it('builds list boxes, and takes select for options and TextInput for TextField', () => {
    const r = tree(
      <View>
        <List name="days" options={['Mon', 'Tue', 'Wed']} value="Tue" />
        <List name="tags" select={['a', 'b', 'c']} multiSelect value={['c', 'a']} className="h-20" />
        <Select name="plan" select={['Starter', 'Team']} value="Team" />
        <TextInput name="email" value="a@b.c" />
        <Field kind="list" name="any" options={['x']} />
      </View>,
    );
    const [days, tags, plan, email, any] = (first<BoxNode>(r).children ?? []) as FieldNode[];
    expect(days?.field).toEqual({
      name: 'days',
      kind: 'list',
      options: ['Mon', 'Tue', 'Wed'],
      value: 'Tue',
      font: 'Inter-Regular',
    });
    expect(tags?.field).toMatchObject({ options: ['a', 'b', 'c'], multiSelect: true, value: ['c', 'a'] });
    expect(tags?.style).toEqual({ height: 60 });
    expect(plan?.field).toMatchObject({ kind: 'select', options: ['Starter', 'Team'], value: 'Team' });
    expect(email?.field).toMatchObject({ kind: 'text', value: 'a@b.c' });
    expect(any?.field).toMatchObject({ kind: 'list', options: ['x'] });
    expect(r.validate()).toEqual([]);
    const several = thrown(() =>
      tree(<List name="x" options={['a', 'b']} value={['a', 'b'] as unknown as string} />),
    );
    expect(several.message).toContain('need multiSelect');
  });

  it('takes a format with its readable keys or their other names', () => {
    const r = tree(
      <View>
        <TextField
          name="fee"
          value="-1234.5"
          format={{
            type: 'number',
            decimals: 0,
            separators: '1.234,56',
            negative: 'parensRed',
            currency: ' €',
            currencyBefore: false,
          }}
        />
        <TextInput
          name="fee2"
          format={{
            type: 'number',
            nDec: 1,
            sepComma: false,
            negStyle: 'ParensBlack',
            currency: '$',
            currencyPrepend: true,
          }}
        />
        <TextInput name="day" value="2026-10-14" format={{ type: 'date', param: 'dd/mm/yyyy' }} />
        <TextField name="zip" format={{ type: 'zip' }} />
      </View>,
    );
    const [fee, fee2, day, zip] = (first<BoxNode>(r).children ?? []) as FieldNode[];
    expect(fee?.field.format).toEqual({
      type: 'number',
      decimals: 0,
      separators: '1.234,56',
      negative: 'parensRed',
      currency: ' €',
      currencyBefore: false,
    });
    expect(fee2?.field.format).toEqual({
      type: 'number',
      decimals: 1,
      separators: '1234.56',
      negative: 'parens',
      currency: '$',
      currencyBefore: true,
    });
    expect(day?.field).toMatchObject({
      value: '2026-10-14',
      format: { type: 'date', pattern: 'dd/mm/yyyy' },
    });
    expect(zip?.field.format).toEqual({ type: 'zip' });
    expect(r.validate()).toEqual([]);
    // The readable key wins.
    const both = tree(
      <TextField name="a" format={{ type: 'number', sepComma: true, separators: '1234,56' }} />,
    );
    expect(first<FieldNode>(both).field.format).toEqual({
      type: 'number',
      decimals: 0,
      separators: '1234,56',
    });
    expect(
      thrown(() => tree(<TextField name="a" format={'number' as unknown as { type: 'number' }} />)).message,
    ).toContain('<TextField format> is an object');
  });

  it('gives a format with the other names their defaults', () => {
    const format = (f: TextFormat) => first<FieldNode>(tree(<TextInput name="a" format={f} />)).field.format;
    // With none of them, the readable defaults (2 decimals, `1,234.56`) apply in the engine.
    expect(format({ type: 'number' })).toEqual({ type: 'number' });
    expect(format({ type: 'number', currency: '$' })).toEqual({ type: 'number', currency: '$' });
    // With any of them: no decimals and no thousands separator.
    for (const f of [
      { currencyPrepend: true },
      { negStyle: 'MinusBlack' as const },
      { nDec: 0 },
      { sepComma: false },
    ]) {
      expect(format({ type: 'number', ...f })).toMatchObject({ decimals: 0, separators: '1234.56' });
    }
    expect(format({ type: 'number', negStyle: 'Red' })).toEqual({
      type: 'number',
      decimals: 0,
      separators: '1234.56',
      negative: 'red',
    });
    expect(format({ type: 'percent', nDec: 1 })).toEqual({
      type: 'percent',
      decimals: 1,
      separators: '1234.56',
    });
    expect(format({ type: 'percent', sepComma: true })).toEqual({
      type: 'percent',
      decimals: 0,
      separators: '1,234.56',
    });
    // The readable keys keep their values.
    expect(format({ type: 'number', nDec: 1, separators: '1.234,56' })).toEqual({
      type: 'number',
      decimals: 1,
      separators: '1.234,56',
    });
    expect(format({ type: 'zip4' })).toEqual({ type: 'zipPlus4' });
    expect(format({ type: 'date', param: 'yyyy-mm-dd' })).toEqual({ type: 'date', pattern: 'yyyy-mm-dd' });
  });

  it('takes a time format by the index of its style', () => {
    const pattern = (param: string | number) =>
      first<FieldNode>(tree(<TextInput name="a" value="18:05" format={{ type: 'time', param }} />)).field
        .format?.pattern;
    expect([0, 1, 2, 3].map(pattern)).toEqual(['HH:MM', 'h:MM tt', 'HH:MM:ss', 'h:MM:ss tt']);
    expect(pattern('1')).toBe('h:MM tt');
    const r = tree(<TextInput name="a" value="18:05" format={{ type: 'time', param: 1 }} />);
    expect(r.validate()).toEqual([]);
    for (const bad of [4, -1, 1.5, '', 'HH:MM', ' 1']) {
      expect(thrown(() => pattern(bad)).message).toContain('param on a time is a style from 0 to 3');
    }
    // A pattern still works, and wins.
    expect(
      first<FieldNode>(tree(<TextInput name="a" format={{ type: 'time', pattern: 'H:MM', param: 9 }} />))
        .field.format,
    ).toEqual({ type: 'time', pattern: 'H:MM' });
  });

  it('puts the names of field sets in front of the names of their fields', () => {
    const r = tree(
      <FieldSet name="applicant" className="p-2" id="who">
        <TextField name="name" />
        <View>
          <FieldSet name="address">
            <TextField name="street" />
            <Radio name="kind" value="home" />
          </FieldSet>
        </View>
        <Checkbox name="consent" />
      </FieldSet>,
    );
    const set = first<BoxNode>(r);
    expect(set).toMatchObject({ type: 'box', id: 'who', style: { padding: { top: 6 } } });
    const names: string[] = [];
    const walk = (n: { type: string; field?: { name: string }; children?: unknown[] }) => {
      if (n.field) names.push(n.field.name);
      for (const c of (n.children ?? []) as (typeof n)[]) walk(c);
    };
    walk(set as never);
    expect(names).toEqual([
      'applicant.name',
      'applicant.address.street',
      'applicant.address.kind',
      'applicant.consent',
    ]);
    expect(r.validate()).toEqual([]);
    // Fields outside it keep their name.
    expect(first<FieldNode>(tree(<TextField name="plain" />)).field.name).toBe('plain');
    expect(thrown(() => tree(<FieldSet name="">x</FieldSet>)).message).toContain('<FieldSet> needs a name');
  });

  it('takes a render prop, as a View does, and names the fields it renders', () => {
    const r = tree(
      <FieldSet
        name="page"
        className="p-2"
        render={({ pageNumber }) => <TextField name={pageNumber === 1 ? 'first' : 'later'} />}
      />,
      { pageCount: 2 },
    );
    expect(r.dynamic).toBe(true);
    const variants = (first<BoxNode>(r).children ?? []) as BoxNode[];
    expect(variants.map((v) => [v.pages, ((v.children ?? []) as FieldNode[])[0]?.field.name])).toEqual([
      [[1], 'page.first'],
      [[2], 'page.later'],
    ]);
    // One page: a plain set, with its style.
    const one = first<BoxNode>(
      tree(<FieldSet name="s" className="p-2" render={() => <TextField name="a" />} />),
    );
    expect(one.style).toEqual({ padding: { top: 6, right: 6, bottom: 6, left: 6 } });
    expect(((one.children ?? []) as FieldNode[])[0]?.field.name).toBe('s.a');
  });
});

describe('notes', () => {
  it('builds a note from its text, with its background colour as its colour', () => {
    const r = tree(
      <View>
        <Note author="Ana" open className="absolute top-2 bg-blue-500">
          Check the totals ({3} rows).
        </Note>
        <Note style={{ backgroundColor: '#ff0000', marginTop: 4 }}>Plain</Note>
        <Note>Yellow</Note>
      </View>,
    );
    const [a, b, c] = (first<BoxNode>(r).children ?? []) as NoteNode[];
    expect(a?.note).toMatchObject({ contents: 'Check the totals (3 rows).', author: 'Ana', open: true });
    expect(a?.note.color).toHaveLength(4);
    expect(a?.style).toEqual({ position: 'absolute', inset: { top: 6 } });
    expect(b).toEqual({
      type: 'note',
      note: { contents: 'Plain', color: [1, 0, 0, 1] },
      style: { margin: { top: 4 } },
    });
    expect(c).toEqual({ type: 'note', note: { contents: 'Yellow' } });
    expect(r.validate()).toEqual([]);
  });

  it('contains only text', () => {
    expect(
      thrown(() =>
        tree(
          <Note>
            <Text>x</Text>
          </Note>,
        ),
      ).message,
    ).toContain('A <Note> contains only text');
    expect(thrown(() => tree(<Note />)).message).toContain('<Note> needs text');
    // Fixed on every page, like a watermark.
    const fixed = tree(
      <Note fixed className="absolute right-0 top-0">
        On every page
      </Note>,
    );
    expect(first<NoteNode>(fixed)).toMatchObject({ type: 'note', break: { fixed: true } });
  });

  it('is a run of its own inside a Text, with its colour', () => {
    const r = tree(
      <Text className="text-[12pt]">
        Total
        <Note author="Ana" className="bg-blue-500">
          Check it ({3} rows).
        </Note>{' '}
        due
        <Link href="https://example.com">
          {' '}
          now<Note>Linked</Note>
        </Link>
      </Text>,
    );
    const runs = first<TextNode>(r).runs;
    expect(runs.map((x) => x.text ?? x.note?.contents)).toEqual([
      'Total',
      'Check it (3 rows).',
      ' due',
      ' now',
      'Linked',
    ]);
    expect(runs[1]).toMatchObject({ note: { contents: 'Check it (3 rows).', author: 'Ana' }, size: 12 });
    expect(runs[1]?.note?.color).toHaveLength(4);
    // A note carries no link: it takes no room to click.
    expect(runs[4]).toEqual({ note: { contents: 'Linked' }, font: runs[0]?.font, size: 12 });
    expect(r.validate()).toEqual([]);
  });

  it('takes no block props or box styles inside a Text', () => {
    const inText = (note: ReactNode) => thrown(() => tree(<Text>a {note}</Text>)).message;
    expect(inText(<Note id="n">b</Note>)).toContain('`id` does not apply to it');
    expect(inText(<Note fixed>b</Note>)).toContain('`fixed` does not apply to it');
    expect(inText(<Note className="p-2">b</Note>)).toContain('it takes only backgroundColor');
    expect(inText(<Note />)).toContain('<Note> needs text');
  });
});

describe('drawings', () => {
  it('serialises Svg primitives to an SVG image', () => {
    const r = tree(
      <Svg width={100} height="20mm" viewBox="0 0 100 50" className="mt-2">
        <Defs>
          <LinearGradient id="g" x1="0" x2="1">
            <Stop offset="0" stopColor="#1e4a99" />
            <Stop offset="1" stopColor="white" />
          </LinearGradient>
        </Defs>
        <G transform="translate(5 5)">
          <Rect width={40} height={20} rx={3} fill="url(#g)" />
          <Circle cx={70} cy={10} r={8} stroke="red" strokeWidth={2} strokeDasharray={[2, 1]} fill="none" />
          <Path d="M0 40 L100 40" stroke="black" />
        </G>
      </Svg>,
    );
    const img = first<ImageNode>(r);
    expect(img.style).toEqual({ width: 100, height: 56.692913, margin: { top: 6 } });
    const svg = decode(img.data);
    expect(svg).toContain('viewBox="0 0 100 50"');
    expect(svg).toContain('<linearGradient id="g" x1="0" x2="1"><stop offset="0" stop-color="#1e4a99"/>');
    expect(svg).toContain('stroke-width="2" stroke-dasharray="2 1"');
    expect(
      thrown(() =>
        tree(
          <Svg width={10} height={10}>
            text
          </Svg>,
        ),
      ).message,
    ).toContain('Text inside <Svg> goes in a <Text>');
    expect(thrown(() => tree(<Rect width={1} height={1} />)).message).toContain('goes inside <Svg>');
  });

  it('takes clipPathUnits on a ClipPath, typed by its exported props', () => {
    const clip: ClipPathProps = { id: 'c', clipPathUnits: 'objectBoundingBox' };
    const defs: DefsProps = {};
    const r = tree(
      <Svg width={10} height={10}>
        <Defs {...defs}>
          <ClipPath {...clip}>
            <Circle cx={0.5} cy={0.5} r={0.5} />
          </ClipPath>
        </Defs>
        <Rect width={10} height={10} clipPath="url(#c)" />
      </Svg>,
    );
    expect(decode(first<ImageNode>(r).data)).toContain('<clipPath id="c" clipPathUnits="objectBoundingBox">');
  });

  it('writes a Marker and the shapes that reference it', () => {
    const arrow: MarkerProps = {
      id: 'arrow',
      viewBox: '0 0 10 10',
      refX: 10,
      refY: 5,
      markerWidth: 6,
      markerHeight: 6,
      orient: 'auto-start-reverse',
      markerUnits: 'strokeWidth',
    };
    const r = tree(
      <Svg width={100} height={40}>
        <Defs>
          <Marker {...arrow}>
            <Path d="M0 0L10 5L0 10Z" fill="context-stroke" />
          </Marker>
          <Marker id="dot" markerWidth={4} markerHeight={4} refX={2} refY={2} orient={30}>
            <Circle cx={2} cy={2} r={2} />
          </Marker>
          <RadialGradient id="r" cx={0.5} cy={0.5} r={0.5} fx={0.3} fy={0.3} fr={0.1}>
            <Stop offset={0} stopColor="white" />
          </RadialGradient>
        </Defs>
        <Polyline
          points={[10, 30, 50, 10, 90, 30]}
          fill="none"
          stroke="black"
          markerStart="url(#arrow)"
          markerMid="url(#dot)"
          markerEnd="url(#arrow)"
        />
      </Svg>,
    );
    const svg = decode(first<ImageNode>(r).data);
    expect(svg).toContain(
      '<marker id="arrow" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse" markerUnits="strokeWidth"><path d="M0 0L10 5L0 10Z" fill="context-stroke"/></marker>',
    );
    expect(svg).toContain('<marker id="dot" markerWidth="4" markerHeight="4" refX="2" refY="2" orient="30">');
    expect(svg).toContain('fx="0.3" fy="0.3" fr="0.1"');
    expect(svg).toContain('marker-start="url(#arrow)" marker-mid="url(#dot)" marker-end="url(#arrow)"/>');
    expect(r.validate()).toEqual([]);
    expect(
      thrown(() =>
        tree(
          <Svg width={10} height={10}>
            <Marker id="m">
              <Stop offset={0} stopColor="red" />
            </Marker>
          </Svg>,
        ),
      ).message,
    ).toContain('<Stop>');
  });

  it('writes Text and Tspan in an Svg as SVG text, with registered faces', () => {
    const span: TspanProps = { dx: 2, baselineShift: 'super', fontSize: 8, fill: 'red' };
    const r = tree(
      <View className="font-[Noto_Sans] text-[11pt]">
        <Svg width={100} height={40}>
          <G fontFamily="Missing, Inter">
            <Text x={[10, 20]} y={30} textAnchor="middle" textLength={50} lengthAdjust="spacingAndGlyphs">
              a &lt; b {'&'} <Tspan {...span}>2</Tspan>
              <Tspan fontWeight="bold" fontStyle="italic">
                c
              </Tspan>
            </Text>
          </G>
          <Text x={5} y={10}>
            noto
          </Text>
        </Svg>
      </View>,
    );
    const img = first<BoxNode>(r).children?.[0] as ImageNode;
    const svg = decode(img.data);
    // The size around the drawing is its text's default.
    expect(svg).toContain('viewBox="0 0 100 40" font-size="11">');
    expect(svg).toContain(
      `<text x="10 20" y="30" text-anchor="middle" textLength="50" lengthAdjust="spacingAndGlyphs" font-family="'Inter'" font-weight="400">a &lt; b &amp; <tspan dx="2" baseline-shift="super" font-size="8" fill="red" font-family="'Inter'" font-weight="400">2</tspan>`,
    );
    // Inter has no bold italic: the italic face, the closest it has.
    expect(svg).toContain(`<tspan font-family="'Inter'" font-weight="400" font-style="italic">c</tspan>`);
    expect(svg).toContain(`<text x="5" y="10" font-family="'Noto Sans'" font-weight="400">noto</text>`);
    // Their fonts are declared.
    expect((r.document.fonts ?? []).map((f) => f.id)).toEqual(
      expect.arrayContaining(['Inter-Regular', 'Inter-Italic', 'NotoSans-Regular']),
    );
    expect(r.validate()).toEqual([]);
  });

  it('writes direction on SVG text and alignmentBaseline on a Tspan', () => {
    const r = tree(
      <Svg width={100} height={40}>
        <Text x={90} y={30} direction="rtl" textAnchor="start">
          a<Tspan alignmentBaseline="hanging">b</Tspan>
        </Text>
      </Svg>,
    );
    const svg = decode((first<ImageNode>(r) as ImageNode).data);
    expect(svg).toContain('<text x="90" y="30" direction="rtl" text-anchor="start"');
    expect(svg).toContain('<tspan alignment-baseline="hanging"');
    expect(r.validate()).toEqual([]);
  });

  it('allows only strings and Tspan in SVG text, and Tspan only there', () => {
    const svg = (children: ReactNode) =>
      thrown(() =>
        tree(
          <Svg width={10} height={10}>
            {children}
          </Svg>,
        ),
      ).message;
    expect(
      svg(
        <Text>
          <Rect width={1} height={1} />
        </Text>,
      ),
    ).toContain('cannot be inside SVG text');
    expect(
      svg(
        <Text>
          <Text>a</Text>
        </Text>,
      ),
    ).toContain('cannot be inside SVG text');
    expect(svg(<Tspan>a</Tspan>)).toContain('<Tspan> goes inside a <Text> in an <Svg>');
    expect(
      thrown(() =>
        tree(
          <Text>
            <Tspan>a</Tspan>
          </Text>,
        ),
      ).message,
    ).toContain('<Tspan> goes inside a <Text> in an <Svg>');
    expect(svg(<Text style={{ color: 'red' }}>a</Text>)).toContain('style applies to <Text> outside <Svg>');
    expect(svg(<Text fontWeight={'heavyish' as never}>a</Text>)).toContain('fontWeight "heavyish"');
  });

  it('keeps the layout props of Svg out of the drawing', () => {
    // A fixed element takes no break, so the layout props come in two drawings.
    const r = tree(
      <>
        <Svg width={10} height={10} className="absolute top-0" fixed debug fill="red">
          <Rect width={1} height={1} />
        </Svg>
        <Svg
          width={10}
          height={10}
          className="mt-2"
          style={{ marginLeft: 4 }}
          pages="first"
          break="avoid"
          href="https://example.com"
          fill="red"
        >
          <Rect width={1} height={1} />
        </Svg>
      </>,
    );
    for (const node of body(r)) {
      expect(decode((node as ImageNode).data)).toBe(
        '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10" fill="red"><rect width="1" height="1"/></svg>',
      );
    }
    expect(body(r)).toHaveLength(2);
  });

  it('records Canvas painting as SVG', () => {
    const r = tree(
      <Canvas
        width={80}
        height={40}
        paint={(p, w, h) => {
          p.save()
            .translate(w / 2, h / 2)
            .circle(0, 0, 10)
            .fill('#c00');
          p.restore().lineWidth(2).dash(3).moveTo(0, 0).lineTo(w, h).stroke('blue');
        }}
      />,
    );
    const svg = decode(first<ImageNode>(r).data);
    expect(svg).toContain(
      '<g transform="translate(40 20)"><path d="M-10 0A10 10 0 1 0 10 0A10 10 0 1 0 -10 0Z" fill="#c00"/></g>',
    );
    expect(svg).toContain(
      '<path d="M0 0L80 40" fill="none" stroke="blue" stroke-width="2" stroke-miterlimit="10" stroke-dasharray="3 3"/>',
    );
  });

  const paint = (fn: (p: Painter) => void): string =>
    decode(first<ImageNode>(tree(<Canvas width={100} height={100} paint={fn} />)).data);

  it('sets the miter limit of mitred strokes, 10 by default', () => {
    const svg = paint((p) => {
      p.rect(0, 0, 10, 10).stroke();
      p.miterLimit(2.5).rect(0, 0, 10, 10).stroke();
      p.lineJoin('round').rect(0, 0, 10, 10).stroke();
      p.rect(0, 0, 10, 10).fill();
    });
    expect(svg).toContain('stroke="black" stroke-width="1" stroke-miterlimit="10"/>');
    expect(svg).toContain('stroke="black" stroke-width="1" stroke-miterlimit="2.5"/>');
    expect(svg).toContain('stroke="black" stroke-width="1" stroke-linejoin="round"/>');
    expect(svg).toContain('<path d="M0 0h10v10h-10Z" fill="black"/>');
  });

  it('clips to the current path until restore, nested clips included', () => {
    const svg = paint((p) => {
      p.save().circle(50, 50, 40).clip();
      p.save().rect(0, 0, 50, 100).clip('evenodd');
      p.rect(0, 0, 100, 100).fill('red');
      p.restore();
      p.rect(0, 0, 100, 100).fill('blue');
      p.restore().rect(0, 0, 10, 10).fill('green');
      // A clip outside save() lasts to the end of the drawing.
      p.clip();
    });
    expect(svg).toContain(
      '<clipPath id="ink-c1"><path d="M10 50A40 40 0 1 0 90 50A40 40 0 1 0 10 50Z"/></clipPath><g clip-path="url(#ink-c1)">' +
        '<clipPath id="ink-c2"><path d="M0 0h50v100h-50Z" clip-rule="evenodd"/></clipPath><g clip-path="url(#ink-c2)">' +
        '<path d="M0 0h100v100h-100Z" fill="red"/></g>' +
        '<path d="M0 0h100v100h-100Z" fill="blue"/></g>' +
        '<path d="M0 0h10v10h-10Z" fill="green"/>' +
        '<clipPath id="ink-c3"></clipPath><g clip-path="url(#ink-c3)"></g></svg>',
    );
  });

  it('writes Canvas text with the canvas font, registered faces and the text options', () => {
    const r = tree(
      <View className="font-[Noto_Sans]">
        <Canvas
          width={200}
          height={100}
          paint={(p) => {
            p.text('A  & <b>', 10, 20);
            const g = p.linearGradient(0, 0, 100, 0).stop(0, 'red').stop(1, 'blue');
            p.font('Inter-Bold', 20).fillColor(g).text('Total', 10, 40, {
              width: 100,
              align: 'right',
              baseline: 'alphabetic',
              characterSpacing: 2,
              wordSpacing: 3,
              underline: true,
              strike: true,
            });
            p.font('Roboto Mono').fontSize(9).strokeColor('#00f').lineWidth(0.5);
            p.text('mono', 0, 0, {
              align: 'center',
              width: 50,
              baseline: 'middle',
              fill: false,
              stroke: true,
            });
            p.font('Helvetica-BoldOblique').text('std', 0, 0);
          }}
        />
      </View>,
    );
    const img = (first<BoxNode>(r).children ?? [])[0] as ImageNode;
    const svg = decode(img.data);
    // The canvas's family, at 12 points, its top at y.
    expect(svg).toContain(
      `<text x="10" y="20" xml:space="preserve" font-family="'Noto Sans'" font-weight="400" font-size="12" dominant-baseline="text-before-edge" fill="black">A  &amp; &lt;b&gt;</text>`,
    );
    expect(svg).toContain(
      `<text x="110" y="40" xml:space="preserve" font-family="'Inter'" font-weight="700" font-size="20" text-anchor="end" dominant-baseline="alphabetic" letter-spacing="2" word-spacing="3" text-decoration="underline line-through" fill="url(#ink-g1)">Total</text>`,
    );
    expect(svg).toContain(
      `<text x="25" y="0" xml:space="preserve" font-family="'Roboto Mono'" font-weight="400" font-size="9" text-anchor="middle" dominant-baseline="middle" fill="none" stroke="#00f" stroke-width="0.5" stroke-miterlimit="10">mono</text>`,
    );
    // A standard name: the canvas's family in its weight and style (Noto Sans has one face), with
    // a warning.
    expect(svg).toContain(`font-family="'Noto Sans'" font-weight="400" font-size="9" dominant-baseline`);
    expect(r.warnings).toContain(
      'Font "Helvetica 700 italic" is not registered; using NotoSans-Regular (Canvas in View).',
    );
    expect((r.document.fonts ?? []).map((f) => f.id)).toEqual(
      expect.arrayContaining(['NotoSans-Regular', 'Inter-Bold', 'RobotoMono-Regular']),
    );
    expect(r.validate()).toEqual([]);
    expect(
      thrown(() => tree(<Canvas width={10} height={10} paint={(p) => p.text('two\nlines', 0, 0)} />)).message,
    ).toContain('painter.text draws one line');
  });

  it("aligns Canvas text with justify as with left: a line doesn't wrap", () => {
    const svg = paint((p) => p.text('a b', 10, 20, { width: 100, align: 'justify' }));
    expect(svg).toContain('<text x="10" y="20" xml:space="preserve"');
    expect(svg).not.toContain('text-anchor');
  });

  it('paints with linear and radial gradients, written once each', () => {
    let shared: CanvasGradient | undefined;
    const svg = paint((p) => {
      const g = p.linearGradient(0, 0, 100, 0).stop(0, 'white').stop(1, '#1e4a99', 0.5);
      shared = g;
      p.rect(0, 0, 50, 50).fill(g);
      p.translate(50, 0).fillColor(g).rect(0, 0, 50, 50).fill();
      const r = p.radialGradient(30, 30, 5, 50, 50, 40).stop(0, 'red').stop(1, '<"&>');
      p.lineWidth(4).circle(50, 50, 20).fillAndStroke(r, g);
      // A stop added after the gradient was used makes a new one.
      g.stop(0.5, 'red');
      p.strokeColor(g).moveTo(0, 0).lineTo(10, 10).stroke();
    });
    expect(shared).toBeDefined();
    expect(svg).toContain(
      '<defs><linearGradient id="ink-g1" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="100" y2="0">' +
        '<stop offset="0" stop-color="white"/><stop offset="1" stop-color="#1e4a99" stop-opacity="0.5"/></linearGradient></defs>' +
        '<path d="M0 0h50v50h-50Z" fill="url(#ink-g1)"/>' +
        '<g transform="translate(50 0)"><path d="M0 0h50v50h-50Z" fill="url(#ink-g1)"/>',
    );
    expect(svg).toContain(
      '<defs><radialGradient id="ink-g2" gradientUnits="userSpaceOnUse" fx="30" fy="30" fr="5" cx="50" cy="50" r="40">' +
        '<stop offset="0" stop-color="red"/><stop offset="1" stop-color="&lt;&quot;&amp;&gt;"/></radialGradient></defs>',
    );
    expect(svg).toContain('fill="url(#ink-g2)" stroke="url(#ink-g1)" stroke-width="4"');
    expect(svg).toContain(
      '<stop offset="0.5" stop-color="red"/></linearGradient></defs><path d="M0 0L10 10" fill="none" stroke="url(#ink-g3)"',
    );
    expect(svg.match(/<linearGradient /g)).toHaveLength(2);
    expect(
      thrown(() => paint((p) => p.rect(0, 0, 1, 1).fill([1, 0, 0] as unknown as string))).message,
    ).toContain('A paint is a colour string, or a gradient');
  });
});

describe('remote images', () => {
  it('allows listed hosts and prefixes, https only for host entries', () => {
    const policy = { allow: ['cdn.example.com', '*.pictures.example.org', 'http://local.test/img/'] };
    expect(remoteAllowed('https://cdn.example.com/a.png', policy)).toBe(true);
    expect(remoteAllowed('http://cdn.example.com/a.png', policy)).toBe(false);
    expect(remoteAllowed('https://a.pictures.example.org/x.jpg', policy)).toBe(true);
    expect(remoteAllowed('https://pictures.example.org/x.jpg', policy)).toBe(false);
    expect(remoteAllowed('https://cdn.example.com.evil.com/a.png', policy)).toBe(false);
    expect(remoteAllowed('http://local.test/img/a.png', policy)).toBe(true);
    expect(remoteAllowed('http://local.test/other.png', policy)).toBe(false);
    expect(remoteAllowed('https://user:pw@cdn.example.com/a.png', policy)).toBe(false);
  });

  it('matches a host entry on the https port, or on the port it names', () => {
    const allowed = (url: string, entry: string) => remoteAllowed(url, { allow: [entry] });
    for (const entry of ['cdn.example.com', 'CDN.example.com:443']) {
      expect(allowed('https://cdn.example.com/a.png', entry)).toBe(true);
      expect(allowed('https://cdn.example.com:443/a.png', entry)).toBe(true);
      expect(allowed('https://cdn.example.com:8443/a.png', entry)).toBe(false);
      expect(allowed('https://cdn.example.com:80/a.png', entry)).toBe(false);
    }
    expect(allowed('https://cdn.example.com:8443/a.png', 'cdn.example.com:8443')).toBe(true);
    expect(allowed('https://cdn.example.com/a.png', 'cdn.example.com:8443')).toBe(false);
    expect(allowed('https://cdn.example.com:8444/a.png', 'cdn.example.com:8443')).toBe(false);
    expect(allowed('https://a.example.org/x.jpg', '*.example.org')).toBe(true);
    expect(allowed('https://a.example.org:9000/x.jpg', '*.example.org')).toBe(false);
    expect(allowed('https://a.example.org:9000/x.jpg', '*.example.org:9000')).toBe(true);
    expect(allowed('https://example.org:9000/x.jpg', '*.example.org:9000')).toBe(false);
  });

  it('matches a URL entry by origin, and by path at a slash', () => {
    const allowed = (url: string, entry: string) => remoteAllowed(url, { allow: [entry] });
    for (const entry of [
      'https://cdn.example.com',
      'https://cdn.example.com/',
      'https://CDN.example.com:443',
    ]) {
      expect(allowed('https://cdn.example.com/a.png', entry)).toBe(true);
      expect(allowed('https://cdn.example.com.evil.net/x', entry)).toBe(false);
      expect(allowed('https://cdn.example.com:8443/x', entry)).toBe(false);
      expect(allowed('http://cdn.example.com/x', entry)).toBe(false);
      expect(allowed('https://cdn.example.comx/x', entry)).toBe(false);
    }
    for (const entry of ['https://cdn.example.com/img', 'https://cdn.example.com/img/']) {
      expect(allowed('https://cdn.example.com/img/a.png', entry)).toBe(true);
      expect(allowed('https://cdn.example.com/img/sub/a.png?v=2', entry)).toBe(true);
      expect(allowed('https://cdn.example.com/imgs/a.png', entry)).toBe(false);
      expect(allowed('https://cdn.example.com/img.png', entry)).toBe(false);
      expect(allowed('https://cdn.example.com/img/../secret.png', entry)).toBe(false);
    }
    expect(allowed('https://cdn.example.com/img', 'https://cdn.example.com/img')).toBe(true);
    expect(allowed('http://localhost:8080/a.png', 'http://localhost:8080')).toBe(true);
    expect(allowed('http://localhost:8081/a.png', 'http://localhost:8080')).toBe(false);
    expect(allowed('http://cdn.example.com:80/a.png', 'http://cdn.example.com')).toBe(true);
    expect(allowed('http://cdn.example.com:8080/a.png', 'http://cdn.example.com')).toBe(false);
    expect(allowed('ftp://cdn.example.com/a.png', 'ftp://cdn.example.com/')).toBe(false);
    expect(allowed('https://cdn.example.com/a.png', 'https://')).toBe(false);
  });

  it('lists allowed URLs for the caller to fetch, one id per URL', () => {
    const r = tree(
      <>
        <Image src="https://cdn.example.com/logo.png" />
        <Image src="https://cdn.example.com/logo.png" />
        <Image src="https://img.example.com/public/a.webp" />
      </>,
    );
    expect(body(r).map((n) => (n as ImageNode).src)).toEqual(['remote-0', 'remote-0', 'remote-1']);
    expect(r.remote).toEqual({
      'remote-0': 'https://cdn.example.com/logo.png',
      'remote-1': 'https://img.example.com/public/a.webp',
    });
    expect(thrown(() => tree(<Image src="https://img.example.com/private/a.png" />)).message).toContain(
      'allowlist',
    );
  });
});

describe('image sources', () => {
  const images = (children: ReactNode, imageIds?: string[]) =>
    renderTemplate(
      <Document>
        <Page>{children}</Page>
      </Document>,
      { fonts, imageIds, remoteImages: { allow: ['cdn.example.com'] }, localImages: true },
    );
  const srcs = (r: ReturnType<typeof images>) =>
    (r.document.pages[0].children ?? []).map((n) => (n as ImageNode).src);

  it('tells file paths from image ids by their form', () => {
    for (const p of [
      '/srv/logo.png',
      './logo.png',
      '../img/logo.png',
      '.\\logo.png',
      'C:\\img\\a.png',
      'file:///a.png',
    ])
      expect(isImagePath(p)).toBe(true);
    for (const p of [
      'logo',
      'logo.png',
      'img/logo.png',
      'https://cdn.example.com/a.png',
      'data:image/png;base64,',
    ])
      expect(isImagePath(p)).toBe(false);
  });

  it('sorts a src the same way for templates and resolved values', () => {
    const kind = (src: unknown) => imageSource(src, (id) => id === './logo.png').kind;
    expect(kind('./logo.png')).toBe('registered');
    expect(kind('./other.png')).toBe('path');
    expect(kind('data:image/png;base64,')).toBe('data');
    expect(kind('https://cdn.example.com/a.png')).toBe('url');
    expect(kind('logo')).toBe('id');
    expect(kind('')).toBe('id');
    expect(kind(new Uint8Array(1))).toBe('bytes');
    expect(kind(new ArrayBuffer(1))).toBe('bytes');
    expect(kind(() => 'logo')).toBe('function');
    expect(kind(Promise.resolve('logo'))).toBe('promise');
    expect(kind({ uri: 'https://cdn.example.com/a.png' })).toBe('request');
    expect(kind(undefined)).toBe('none');
  });

  it('lists file paths and file URLs to load, one id per path; other strings stay ids', () => {
    const r = images(
      <>
        <Image src="./logo.png" />
        <Image src="/srv/img/photo.jpg" />
        <Image src="./logo.png" />
        <Image src="file:///srv/img/a.png" />
        <Image src="logo.png" />
      </>,
    );
    expect(srcs(r)).toEqual(['file-0', 'file-1', 'file-0', 'file-2', 'logo.png']);
    expect(r.pending).toEqual({
      'file-0': { kind: 'file', path: './logo.png' },
      'file-1': { kind: 'file', path: '/srv/img/photo.jpg' },
      'file-2': { kind: 'file', path: 'file:///srv/img/a.png' },
    });
  });

  it('keeps paths as image ids unless local images are allowed', () => {
    const r = renderTemplate(
      <Document>
        <Page>
          <Image src="./logo.png" />
          <Image src="/srv/img/photo.jpg" />
        </Page>
      </Document>,
      { fonts },
    );
    expect(srcs(r)).toEqual(['./logo.png', '/srv/img/photo.jpg']);
    expect(r.pending).toEqual({});
  });

  it('keeps a string that names a passed-in image as that id, path or URL alike', () => {
    const r = images(
      <>
        <Image src="./logo.png" />
        <Image src="https://img.example.com/private/a.png" />
      </>,
      ['./logo.png', 'https://img.example.com/private/a.png'],
    );
    expect(srcs(r)).toEqual(['./logo.png', 'https://img.example.com/private/a.png']);
    expect(r.pending).toEqual({});
    expect(r.remote).toEqual({});
  });

  it('lists allowed requests and checks their uri against the allowlist', () => {
    const request = { uri: 'https://cdn.example.com/a.png', method: 'POST', headers: { a: 'b' }, body: 'x' };
    const r = images(
      <>
        <Image src={request} />
        <Image src={request} />
      </>,
    );
    expect(srcs(r)).toEqual(['request-0', 'request-0']);
    expect(r.pending).toEqual({ 'request-0': { kind: 'request', request } });
    expect(thrown(() => images(<Image src={{ uri: 'https://evil.example.org/a.png' }} />)).message).toBe(
      '<Image src={{ uri: "https://evil.example.org/a.png" }}>: this URL is not in the remoteImages allowlist. Add its origin to the option remoteImages.allow, or pass the bytes.',
    );
    expect(thrown(() => images(<Image src={{ uri: '' }} />)).message).toContain('a request needs a uri');
    const hinted = (src: ImageSrc) =>
      thrown(() =>
        renderTemplate(
          <Document>
            <Page>
              <Image src={src} />
            </Page>
          </Document>,
          { fonts, remoteImagesElsewhere: 'the INKOXIDE_REMOTE_IMAGES variable' },
        ),
      ).message;
    expect(hinted('https://evil.example.org/a.png')).toBe(
      '<Image src="https://evil.example.org/a.png">: this URL is not in the remoteImages allowlist. Add its origin to the option remoteImages.allow (or the INKOXIDE_REMOTE_IMAGES variable), or pass the bytes.',
    );
    expect(hinted({ uri: 'https://evil.example.org/a.png' })).toContain(
      'remoteImages.allow (or the INKOXIDE_REMOTE_IMAGES variable)',
    );
  });

  it('lists functions and promises without calling or awaiting them', async () => {
    let calls = 0;
    const fn = () => {
      calls++;
      return 'logo';
    };
    const promise = Promise.resolve(new Uint8Array([1]));
    const r = images(
      <>
        <Image src={fn} />
        <Image src={promise} />
        <Image src={fn} />
      </>,
    );
    expect(srcs(r)).toEqual(['deferred-0', 'deferred-1', 'deferred-0']);
    expect(calls).toBe(0);
    expect(r.pending['deferred-0']).toEqual({ kind: 'deferred', src: fn });
    const p = r.pending['deferred-1'];
    expect(p?.kind === 'deferred' && (await p.src)).toEqual(new Uint8Array([1]));
  });

  it('takes an ArrayBuffer as bytes, one id per buffer', () => {
    const buffer = new Uint8Array([1, 2, 3]).buffer;
    const r = images(
      <>
        <Image src={buffer} />
        <Image src={buffer} />
      </>,
    );
    expect(srcs(r)).toEqual(['image-0', 'image-0']);
    expect(r.images).toEqual({ 'image-0': new Uint8Array([1, 2, 3]) });
    expect(r.rebuild(2).images['image-0']).toBe(r.images['image-0']);
  });

  it('never generates an id the caller passes in', () => {
    const r = images(
      <>
        <Image src="./a.png" />
        <Image src="./b.png" />
        <Image src="file-0" />
      </>,
      ['file-0', 'file-2'],
    );
    expect(srcs(r)).toEqual(['file-1', 'file-3', 'file-0']);
  });

  it('keeps image ids across rebuilds; images a rebuild adds get new ones', () => {
    const fn = () => 'logo';
    const r = images(
      <>
        <View render={({ pageNumber }) => <Image src={pageNumber === 1 ? './a.png' : './b.png'} />} />
        <Image src="./c.png" />
        <Image src={fn} />
        <View render={() => <Image src={{ uri: 'https://cdn.example.com/a.png', headers: { A: 'b' } }} />} />
      </>,
    );
    expect(Object.keys(r.pending)).toEqual(['file-0', 'file-1', 'deferred-0', 'request-0']);
    const again = r.rebuild(2);
    expect(again.pending).toEqual({
      'file-0': { kind: 'file', path: './a.png' },
      'file-2': { kind: 'file', path: './b.png' },
      'file-1': { kind: 'file', path: './c.png' },
      'deferred-0': { kind: 'deferred', src: fn },
      'request-0': r.pending['request-0'],
    });
  });

  it('does not leave a rejected promise unhandled when the tree is never rendered', async () => {
    const unhandled: unknown[] = [];
    const onUnhandled = (e: unknown) => unhandled.push(e);
    process.on('unhandledRejection', onUnhandled);
    try {
      images(<Image src={Promise.reject(new Error('gone'))} />);
      await new Promise((ok) => setTimeout(ok, 10));
    } finally {
      process.off('unhandledRejection', onUnhandled);
    }
    expect(unhandled).toEqual([]);
  });
});

describe('debug', () => {
  it('puts debug on the nodes, rows and cells that set it, and nowhere else', () => {
    const r = renderTemplate(
      <Document>
        <Page>
          <Header debug>
            <Text>Head</Text>
          </Header>
          <View debug className="m-2 p-2">
            <Text debug>
              Hi <Text debug>inline</Text>
            </Text>
            <Text>plain</Text>
          </View>
          <Image debug src="logo" />
          <Svg debug width={10} height={10} />
          <Canvas debug width={10} height={10} paint={() => {}} />
          <Spacer debug h={4} />
          <Checkbox debug name="ok" />
          <Table debug columns={[1]}>
            <Row debug>
              <Cell debug>
                <Text>cell</Text>
              </Cell>
            </Row>
            <Row>
              <Cell>
                <Text>cell</Text>
              </Cell>
            </Row>
          </Table>
          <View debug={false} />
        </Page>
      </Document>,
      { fonts, imageIds: ['logo'] },
    );
    const page = r.document.pages[0];
    expect(page.debug).toBeUndefined();
    expect((page.header as BoxNode).debug).toBe(true);
    const nodes = page.children ?? [];
    expect(nodes.map((n) => ('debug' in n ? n.debug : undefined))).toEqual([
      true,
      true,
      true,
      true,
      true,
      true,
      true,
      undefined,
    ]);
    // Inline text has no box of its own: only the paragraph is outlined.
    const view = nodes[0] as BoxNode;
    expect(view.children?.map((n) => ('debug' in n ? n.debug : undefined))).toEqual([true, undefined]);
    const table = nodes[6] as TableNode;
    expect(table.rows.map((row) => [row.debug, row.cells[0]?.debug])).toEqual([
      [true, true],
      [undefined, undefined],
    ]);
  });

  it('sets debug on the page from <Page debug> and on every page from <Document debug>', () => {
    const one = renderTemplate(
      <Document>
        <Page debug />
        <Page />
      </Document>,
      { fonts },
    );
    expect(one.document.pages.map((p) => p.debug)).toEqual([true, undefined]);
    const all = renderTemplate(
      <Document debug>
        <Page />
        <Page />
      </Document>,
      { fonts },
    );
    expect(all.document.pages.map((p) => p.debug)).toEqual([true, true]);
  });

  it('outlines both the per-page wrapper and its variants', () => {
    const r = tree(<View debug className="m-2" render={({ pageNumber }) => <Text>{pageNumber}</Text>} />, {
      pageCount: 2,
    });
    const wrapper = first<BoxNode>(r);
    expect(wrapper.debug).toBe(true);
    expect((wrapper.children as BoxNode[]).map((v) => v.debug)).toEqual([true, true]);
  });
});

describe('Image inside Text', () => {
  it('is a run of its own, sized in points or em and placed by verticalAlign', () => {
    const r = tree(
      <Text style={{ fontSize: 12 }}>
        a <Image src="logo" />
        <Image src="logo" style={{ height: '2em', verticalAlign: 'middle' }} alt="Logo" />
        <Image src="logo" style={{ width: 30, height: 10 }} fit="cover" className="object-left" />
        <Image src="logo" style={{ verticalAlign: 'text-top' }} />
        <Image src="logo" style={{ verticalAlign: -3 }} />
        <Text style={{ verticalAlign: 'super' }}>
          <Image src="logo" />
        </Text>
        <Link href="#x">
          <Image src="data:image/png;base64,AAAA" />
        </Link>
        b
      </Text>,
    );
    const runs = first<TextNode>(r).runs;
    expect(runs[0]).toMatchObject({ text: 'a ', size: 12 });
    const images = runs.filter((run) => run.image);
    expect(images.map((run) => [run.image, run.verticalAlign, run.size])).toEqual([
      [{ src: 'logo' }, undefined, 12],
      [{ src: 'logo', height: 24, alt: 'Logo' }, 'middle', 12],
      [
        { src: 'logo', width: 30, height: 10, fit: 'cover', objectPosition: { x: 'left', y: 'center' } },
        undefined,
        12,
      ],
      [{ src: 'logo' }, 'textTop', 12],
      [{ src: 'logo' }, -3, 12],
      [{ src: 'logo' }, 'super', 12],
      [{ data: 'data:image/png;base64,AAAA' }, undefined, 12],
    ]);
    expect(images[6]?.link).toBe('#x');
    expect(images.every((run) => run.text === undefined && run.font === 'Inter-Regular')).toBe(true);
    expect(runs[runs.length - 1]).toMatchObject({ text: 'b' });
  });

  it('takes the Tailwind classes align-text-top and align-text-bottom', () => {
    const r = tree(
      <Text>
        <Image src="logo" className="align-text-top" />
        <Image src="logo" className="align-text-bottom" />
      </Text>,
    );
    expect(first<TextNode>(r).runs.map((run) => run.verticalAlign)).toEqual(['textTop', 'textBottom']);
  });

  it('takes no box styles, percentages or block props', () => {
    const inText = (img: ReactNode) => () => tree(<Text>{img}</Text>);
    expect(thrown(inText(<Image src="logo" style={{ margin: 4 }} />)).message).toContain(
      'An <Image> inside <Text> is part of the line: it takes width, height, objectFit, objectPosition and verticalAlign, not "marginTop',
    );
    expect(thrown(inText(<Image src="logo" style={{ width: '50%' }} />)).message).toContain(
      'takes its width in points or em, not 50%',
    );
    expect(thrown(inText(<Image src="logo" bookmark="x" />)).message).toContain(
      '`bookmark` does not apply to it',
    );
    expect(thrown(inText(<Image src="logo" href="https://example.com" />)).message).toContain(
      'Wrap it in a <Link>',
    );
    expect(
      thrown(inText(<Image src="logo" style={{ verticalAlign: 'sideways' as never }} />)).message,
    ).toContain('verticalAlign "sideways" is not supported');
  });
});
