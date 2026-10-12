import type { BoxNode, FieldNode, ImageNode, TableNode, TextNode } from '@inkoxide/schema';
import { parseColor } from '@inkoxide/tailwind';
import type { ReactNode } from 'react';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  Canvas,
  Cell,
  Checkbox,
  Circle,
  Defs,
  Document,
  Field,
  Footer,
  G,
  Header,
  Image,
  isImagePath,
  LinearGradient,
  Link,
  PAGE_SIZES,
  Page,
  PageBreak,
  PageNumber,
  Path,
  Radio,
  Rect,
  remoteAllowed,
  renderTemplate,
  Row,
  Select,
  Spacer,
  Stop,
  Svg,
  Table,
  Text,
  TextField,
  TotalPages,
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
    expect(thrown(() => tree(<View style={{ width: '3ex' as never }} />)).message).toContain('not a length');
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
      '"50vw" depends on the font or page size',
    );
  });

  it('rejects lengths and style numbers that are not finite, which JSON would drop', () => {
    const huge = '9'.repeat(400);
    for (const unit of ['mm', 'vw', 'em'] as const) {
      const width = `${huge}${unit}` as `${number}${typeof unit}`;
      expect(thrown(() => tree(<View style={{ width }} />)).message).toContain(
        `"${huge}${unit}" is too large a length`,
      );
    }
    expect(thrown(() => tree(<View style={{ width: 100 / 0 }} />)).message).toBe(
      'Invalid style prop on <View>: width: Infinity is not a finite number',
    );
    expect(thrown(() => tree(<View style={{ marginTop: Number.NaN }} />)).message).toContain(
      'marginTop: NaN is not a finite number',
    );
    expect(thrown(() => tree(<View style={{ transform: `rotate(${huge}deg)` }} />)).message).toContain(
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
    const first = (style: Record<string, unknown>) => {
      const [p] = tree(<View style={{ display: 'grid', ...style }} />).validate();
      return p && `${p.path}: ${p.message}`;
    };
    expect(first({ gridTemplateColumns: ['1000001fr'] })).toBe(
      'pages[0].children[0].style.gridTemplateColumns[0]: must be a flexible size, a share of the free space, of at most 1000000fr, like "1fr"',
    );
    expect(first({ gridTemplateColumns: ['20000%'] })).toBe(
      'pages[0].children[0].style.gridTemplateColumns[0]: must be a percentage within ±10000%, like "50%"',
    );
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
    ).toContain('textDecoration "overline" is not');
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
      'objectFit applies to <Image>, not <View>',
    );
    expect(
      thrown(() => tree(<Image src="logo" style={{ objectFit: 'scale-down' as never }} />)).message,
    ).toContain('objectFit "scale-down" is not fill, contain, cover or none');
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
      'objectPosition applies to <Image>, not <View>',
    );
    expect(thrown(() => tree(<Text className="object-top">x</Text>)).message).toContain(
      'objectPosition applies to <Image>, not <Text>',
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
    const size = (s: unknown) => tree(null, { page: { size: s } }).document.pages[0].size;
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
    const size = (s: unknown) => tree(null, { page: { size: s } }).document.pages[0].size;
    const letter = { width: 612, height: 792 };
    expect(size('letter')).toEqual(letter);
    expect(size('LETTER')).toEqual(letter);
    expect(size('a4-landscape')).toEqual({ width: 841.89, height: 595.28 });
    expect(size('SRA2-Landscape')).toEqual({ width: 1814.17, height: 1275.59 });
    expect(size('id1-landscape')).toEqual({ width: 242.65, height: 153.01 });
  });

  it('sizes pages from [width, height], [width] and a bare length', () => {
    const size = (s: unknown, page: Record<string, unknown> = {}) =>
      tree(null, { page: { size: s, ...page } }).document.pages[0].size;
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
    expect(thrown(() => tree(null, { page: { size: ['10'] } })).message).toContain('"10" is not a length');
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
    ).toContain('Text inside <Svg>');
    expect(thrown(() => tree(<Rect width={1} height={1} />)).message).toContain('goes inside <Svg>');
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
      '<path d="M0 0L80 40" fill="none" stroke="blue" stroke-width="2" stroke-dasharray="3 3"/>',
    );
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
      '<Image src={{ uri: "https://evil.example.org/a.png" }}>: this URL is not in the remoteImages allowlist. Add its origin to the render option remoteImages.allow (or the INKOXIDE_REMOTE_IMAGES variable), or pass the bytes.',
    );
    expect(thrown(() => images(<Image src={{ uri: '' }} />)).message).toContain('a request needs a uri');
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
