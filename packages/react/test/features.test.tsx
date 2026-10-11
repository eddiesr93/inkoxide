import type { BoxNode, FieldNode, ImageNode, TextNode } from '@inkoxide/schema';
import { parseColor } from '@inkoxide/tailwind';
import type { ReactNode } from 'react';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  Canvas,
  Checkbox,
  Circle,
  Defs,
  Document,
  Field,
  Footer,
  G,
  Image,
  LinearGradient,
  Page,
  Path,
  Radio,
  Rect,
  remoteAllowed,
  renderTemplate,
  Select,
  Stop,
  Svg,
  TemplateError,
  Text,
  TextField,
  usePage,
  View,
} from '../src/index.ts';
import { registry, setup } from './helpers.ts';

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
const body = (r: ReturnType<typeof tree>) => r.document.pages[0].children ?? [];
const first = <T,>(r: ReturnType<typeof tree>) => body(r)[0] as T;
const decode = (data: string | undefined): string =>
  Buffer.from((data ?? '').replace(/^data:image\/svg\+xml;base64,/, ''), 'base64').toString('utf8');

function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}

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

  it('accepts the wrap, break and minPresenceAhead aliases and weight names, and explains fixed', () => {
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
    expect(thrown(() => tree(<View {...({ fixed: true } as object)} />)).message).toContain(
      'put content that repeats',
    );
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
      </>,
    );
    const [a, b, c] = (r.document.pages[0].children ?? []) as BoxNode[];
    expect(a?.style).toEqual({ flexGrow: 1, flexShrink: 1, flexBasis: '0%' });
    expect(b?.style).toEqual({ flexGrow: 2, flexShrink: 3, flexBasis: 28.346457 });
    expect(c?.style).toEqual({ position: 'absolute', inset: { top: 0, right: 28.346457 } });
    expect(thrown(() => tree(<View style={{ objectPosition: 'top' } as never} />)).message).toContain(
      'unknown style property "objectPosition"',
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
