import { readFileSync } from 'node:fs';
import { inspect } from 'node:util';

import type { BoxNode, TableNode, TextNode } from '@inkoxide/schema';
import { createContext, lazy, type ReactNode, Suspense, use, useContext } from 'react';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  Cell,
  Document,
  FontRegistry,
  Footer,
  Header,
  Image,
  Link,
  Page,
  PageNumber,
  Row,
  renderTemplate,
  Spacer,
  Table,
  Text,
  TotalPages,
  View,
} from '../src/index.ts';
import { hostConfig } from '../src/reconciler/host-config.ts';
import { body, first, registry, setup, thrown } from './helpers.ts';

beforeAll(setup);

const fonts = registry();
const render = (children: ReactNode, pageClass = 'text-[10pt]') =>
  renderTemplate(
    <Document lang="en-US">
      <Page className={pageClass}>{children}</Page>
    </Document>,
    { fonts },
  );

describe('text inheritance and runs', () => {
  it('cascades text properties from Page through View to nested Text, flattened onto runs', () => {
    const r = render(
      <View className="text-red-500">
        <Text className="font-bold">
          Name: <Text className="font-normal text-[8pt]">Ann</Text>
        </Text>
      </View>,
      'text-[11pt] text-black',
    );
    const text = (first<BoxNode>(r).children?.[0] ?? {}) as TextNode;
    expect(text.runs).toEqual([
      { text: 'Name: ', font: 'Inter-Bold', size: 11, color: [0.982661, 0.171797, 0.21307, 1] },
      { text: 'Ann', font: 'Inter-Regular', size: 8, color: [0.982661, 0.171797, 0.21307, 1] },
    ]);
  });

  it('does not inherit layout or visual properties', () => {
    const r = render(
      <View className="p-4 bg-white">
        <View>
          <Text>x</Text>
        </View>
      </View>,
    );
    const outer = first<BoxNode>(r);
    expect(outer.style?.padding).toEqual({ top: 12, right: 12, bottom: 12, left: 12 });
    expect((outer.children ?? [])[0]).toEqual({ type: 'box', children: [expect.anything()] });
  });

  it('merges adjacent strings with the same style into one run', () => {
    const name = 'Ann';
    const r = render(
      <Text>
        Hello, {name}! You have {3} messages.
      </Text>,
    );
    expect(first<TextNode>(r).runs).toHaveLength(1);
    expect(first<TextNode>(r).runs[0]?.text).toBe('Hello, Ann! You have 3 messages.');
  });

  it('puts text-node properties on the node and resolves length line heights against the size', () => {
    const r = render(<Text className="text-[12pt] leading-6 text-justify truncate hyphens-auto">x</Text>);
    expect(first<TextNode>(r).style).toEqual({
      textAlign: 'justify',
      lineHeight: 1.5,
      maxLines: 1,
      ellipsis: true,
      hyphenate: true,
    });
  });

  it('converts em letter spacing to points with the run size', () => {
    const r = render(<Text className="text-[10pt] tracking-wide">x</Text>);
    expect(first<TextNode>(r).runs[0]?.letterSpacing).toBe(0.25);
  });

  it('applies locale-aware text-transform', () => {
    const r = render(<Text className="uppercase">café and naïve</Text>);
    expect(first<TextNode>(r).runs[0]?.text).toBe('CAFÉ AND NAÏVE');
  });

  it('cases text in a lang Intl does not take by the default rules', () => {
    const doc = renderTemplate(
      <Document lang="en_US">
        <Page>
          <Text style={{ textTransform: 'uppercase' }}>istanbul</Text>
          <Text lang="!!" className="capitalize">
            hello world
          </Text>
          <Text lang="tr" className="uppercase">
            istanbul
          </Text>
        </Page>
      </Document>,
      { fonts },
    );
    expect(body(doc).map((n) => (n as TextNode).runs[0]?.text)).toEqual([
      'ISTANBUL',
      'Hello World',
      'İSTANBUL',
    ]);
  });

  it('supports links on nested runs and on blocks, and bookmarks', () => {
    const r = render(
      <Text id="intro" bookmark="Introduction" href="#intro">
        See <Text href="https://example.com">the website</Text>
      </Text>,
    );
    const t = first<TextNode>(r);
    expect(t.id).toBe('intro');
    expect(t.link).toBe('#intro');
    expect(t.bookmark).toEqual({ title: 'Introduction' });
    expect(t.runs[1]?.link).toBe('https://example.com');
  });

  it('makes a Link inside Text a linked run without adding any style', () => {
    const r = render(
      <Text className="text-[#ff0000]">
        See <Link href="https://example.com">the website</Link> or{' '}
        <Link src="#totals" className="underline">
          the <Text className="font-bold">totals</Text>
        </Link>
        .
      </Text>,
    );
    const red = [1, 0, 0, 1];
    expect(first<TextNode>(r).runs).toEqual([
      { text: 'See ', font: 'Inter-Regular', size: 10, color: red },
      { text: 'the website', font: 'Inter-Regular', size: 10, color: red, link: 'https://example.com' },
      { text: ' or ', font: 'Inter-Regular', size: 10, color: red },
      { text: 'the ', font: 'Inter-Regular', size: 10, color: red, decoration: 'underline', link: '#totals' },
      { text: 'totals', font: 'Inter-Bold', size: 10, color: red, decoration: 'underline', link: '#totals' },
      { text: '.', font: 'Inter-Regular', size: 10, color: red },
    ]);
  });

  it('makes a Link outside Text a box with a link, like a View with href', () => {
    const r = render(
      <Link src="https://example.com" className="p-2" bookmark="Website">
        Example
        <Text>More</Text>
      </Link>,
    );
    const box = first<BoxNode>(r);
    expect(box).toMatchObject({
      type: 'box',
      link: 'https://example.com',
      bookmark: { title: 'Website' },
      style: { padding: { top: 6, right: 6, bottom: 6, left: 6 } },
    });
    const [label, more] = (box.children ?? []) as TextNode[];
    expect(label?.runs).toEqual([{ text: 'Example', font: 'Inter-Regular', size: 10, color: [0, 0, 0, 1] }]);
    expect(more?.runs[0]?.link).toBeUndefined();
  });

  it('turns bare strings in a View into an anonymous text node', () => {
    const r = render(<View className="text-center">Hello</View>);
    const t = first<BoxNode>(r).children?.[0] as TextNode;
    expect(t.type).toBe('text');
    expect(t.style).toEqual({ textAlign: 'center' });
  });

  it('works with hooks and context', () => {
    const Ctx = createContext('default');
    function Name() {
      return <Text>{useContext(Ctx)}</Text>;
    }
    const r = render(
      <Ctx.Provider value="from context">
        <Name />
      </Ctx.Provider>,
    );
    expect(first<TextNode>(r).runs[0]?.text).toBe('from context');
  });

  it('applies the style prop after className', () => {
    const r = render(
      <View className="p-4" style={{ padding: { top: 2 }, backgroundColor: '#ff0000' }}>
        <Text style={{ color: [0, 0, 1, 1], fontSize: 20 }}>x</Text>
      </View>,
    );
    const v = first<BoxNode>(r);
    expect(v.style?.padding).toEqual({ top: 2, right: 12, bottom: 12, left: 12 });
    expect(v.style?.backgroundColor).toEqual([1, 0, 0, 1]);
    expect(((v.children ?? [])[0] as TextNode).runs[0]).toMatchObject({ size: 20, color: [0, 0, 1, 1] });
  });

  it('gives borders without a colour the text colour (currentColor)', () => {
    const r = render(<View className="border text-[#ff0000]" />);
    expect(first<BoxNode>(r).style?.borderColor).toEqual({
      top: [1, 0, 0, 1],
      right: [1, 0, 0, 1],
      bottom: [1, 0, 0, 1],
      left: [1, 0, 0, 1],
    });
  });
});

describe('superscript, subscript and font features', () => {
  const runs = (r: ReturnType<typeof render>) =>
    first<TextNode>(r).runs.map(({ text, size, verticalAlign, fontFeatures }) => ({
      text,
      size,
      ...(verticalAlign && { verticalAlign }),
      ...(fontFeatures && { fontFeatures }),
    }));

  it('puts a nested Text on a raised or lowered baseline, with what is inside it', () => {
    const r = render(
      <Text>
        x<Text style={{ verticalAlign: 'super', fontSize: 6 }}>2</Text> H
        <Text className="align-sub text-[6pt]">
          2<Text className="font-bold">!</Text>
        </Text>
        O <Text style={{ verticalAlign: 'baseline' }}>y</Text>
      </Text>,
    );
    expect(runs(r)).toEqual([
      { text: 'x', size: 10 },
      { text: '2', size: 6, verticalAlign: 'super' },
      { text: ' H', size: 10 },
      { text: '2', size: 6, verticalAlign: 'sub' },
      { text: '!', size: 6, verticalAlign: 'sub' },
      { text: 'O y', size: 10 },
    ]);
  });

  it('applies verticalAlign on the outer Text to all its runs', () => {
    const r = render(
      <Text style={{ verticalAlign: 'super' }}>
        a<Text style={{ verticalAlign: 'baseline' }}>b</Text>
      </Text>,
    );
    expect(runs(r)).toEqual([
      { text: 'a', size: 10, verticalAlign: 'super' },
      { text: 'b', size: 10 },
    ]);
  });

  it("keeps a cell's verticalAlign off its text, and a run's off a cell", () => {
    const r = render(
      <Table columns={[50]}>
        <Row>
          <Cell className="align-middle">
            <Text>x</Text>
          </Cell>
        </Row>
      </Table>,
    );
    const cell = first<TableNode>(r).rows[0]?.cells[0];
    expect(cell?.style?.verticalAlign).toBe('middle');
    const text = cell?.children?.[0] as TextNode | undefined;
    expect(text?.runs[0]).toEqual({ text: 'x', font: 'Inter-Regular', size: 10, color: [0, 0, 0, 1] });
    const e = thrown(() =>
      render(
        <Table columns={[50]}>
          <Row>
            <Cell className="align-super" />
          </Row>
        </Table>,
      ),
    );
    expect(e.message).toMatch(
      /<Cell>: verticalAlign "super" \(from className="align-super"\).*top, middle or bottom/,
    );
    const nested = thrown(() =>
      render(
        <Text>
          a<Text style={{ verticalAlign: 'middle' }}>b</Text>
        </Text>,
      ),
    );
    expect(nested.message).toMatch(/inline run: only text styles apply, not "verticalAlign"/);
    expect(() => render(<Text style={{ verticalAlign: 'sup' as 'super' }}>a</Text>)).toThrow(
      /verticalAlign "sup" is not supported/,
    );
  });

  it('reads fontFeatureSettings in every form, inherited, replaced by a Text that sets its own', () => {
    const r = render(
      <View style={{ fontFeatureSettings: "'tnum' 1, 'liga' off" }}>
        <Text>
          a<Text style={{ fontFeatureSettings: ['smcp'] }}>b</Text>
          <Text style={{ fontFeatureSettings: { ss01: true, salt: 2 } }}>c</Text>
          <Text style={{ fontFeatureSettings: 'normal' }}>d</Text>
        </Text>
      </View>,
    );
    const text = (first<BoxNode>(r).children?.[0] ?? {}) as TextNode;
    expect(text.runs.map((x) => [x.text, x.fontFeatures])).toEqual([
      ['a', { tnum: 1, liga: 0 }],
      ['b', { smcp: 1 }],
      ['c', { ss01: 1, salt: 2 }],
      ['d', undefined],
    ]);
  });

  it('combines fontVariantNumeric and Tailwind numeric classes, with fontFeatureSettings winning', () => {
    const r = render(
      <Text className="tabular-nums slashed-zero">
        1<Text style={{ fontFeatureSettings: { tnum: 0 } }}>2</Text>
        <Text className="font-features-['ss01']">3</Text>
        <Text style={{ fontVariantNumeric: 'oldstyle-nums' }}>4</Text>
      </Text>,
    );
    expect(first<TextNode>(r).runs.map((x) => [x.text, x.fontFeatures])).toEqual([
      ['1', { tnum: 1, zero: 1 }],
      ['2', { tnum: 0, zero: 1 }],
      ['3', { tnum: 1, zero: 1, ss01: 1 }],
      ['4', { onum: 1 }],
    ]);
  });

  it('merges neighbouring runs with the same features, in any order', () => {
    const r = render(
      <Text style={{ fontFeatureSettings: { tnum: 1, zero: 1 } }}>
        a<Text style={{ fontFeatureSettings: { zero: 1, tnum: 1 } }}>b</Text>
      </Text>,
    );
    expect(runs(r)).toEqual([{ text: 'ab', size: 10, fontFeatures: { tnum: 1, zero: 1 } }]);
  });

  it('rejects settings that are not features', () => {
    expect(() => render(<Text style={{ fontFeatureSettings: 'tnum' }}>a</Text>)).toThrow(
      /fontFeatureSettings: tnum \(expected a quoted tag/,
    );
    expect(() => render(<Text style={{ fontFeatureSettings: { tnums: 1 } }}>a</Text>)).toThrow(
      /"tnums" is not a feature tag/,
    );
  });
});

describe('textWrap', () => {
  it('cascades from the page to its paragraphs, and a Text sets its own', () => {
    const r = render(
      <View>
        <Text>a</Text>
        <Text style={{ textWrap: 'wrap' }}>b</Text>
        <Text className="text-wrap">c</Text>
      </View>,
      'text-[10pt] text-pretty',
    );
    const texts = (first<BoxNode>(r).children ?? []) as TextNode[];
    expect(texts.map((t) => t.style?.textWrap)).toEqual(['pretty', undefined, undefined]);
    const plain = render(<Text>a</Text>);
    expect(first<TextNode>(plain).style?.textWrap).toBeUndefined();
  });

  it('rejects balance, which is reserved', () => {
    expect(() => render(<Text style={{ textWrap: 'balance' }}>a</Text>)).toThrow(
      /textWrap "balance" is not supported/,
    );
    expect(() => render(<Text className="text-balance">a</Text>)).toThrow(/text-balance/);
  });
});

describe('floats', () => {
  it('passes float and clear to the box, from style and classes', () => {
    const r = render(
      <View>
        <Image src="logo" style={{ float: 'left', width: 40 }} />
        <View className="float-right w-10" />
        <Text style={{ clear: 'both' }}>after</Text>
      </View>,
    );
    const kids = (first<BoxNode>(r).children ?? []) as BoxNode[];
    expect(kids.map((k) => [k.style?.float, k.style?.clear])).toEqual([
      ['left', undefined],
      ['right', undefined],
      [undefined, 'both'],
    ]);
  });

  it('rejects other values, and float on a run', () => {
    expect(() => render(<View style={{ float: 'start' as 'left' }} />)).toThrow(/float: start/);
    expect(() => render(<View className="float-start" />)).toThrow(/float-start/);
    expect(() =>
      render(
        <Text>
          a<Text style={{ float: 'left' }}>b</Text>
        </Text>,
      ),
    ).toThrow(/inline run: only text styles apply, not "float"/);
  });
});

describe('placeholders', () => {
  it('emits pageNumber and totalPages runs inside Text', () => {
    const r = render(
      <Text>
        Page <PageNumber /> of <TotalPages />
      </Text>,
    );
    const runs = first<TextNode>(r).runs;
    expect(runs.map((x) => x.placeholder ?? x.text)).toEqual(['Page ', 'pageNumber', ' of ', 'totalPages']);
    expect(runs[1]).not.toHaveProperty('text');
  });

  it('rejects placeholders outside Text with the component stack', () => {
    function Footnote() {
      return (
        <View>
          <PageNumber />
        </View>
      );
    }
    const e = thrown(() => render(<Footnote />));
    expect(e.message).toContain('<PageNumber> must be inside a <Text>');
    expect(e.componentStack).toContain('in Footnote');
  });
});

describe('hyphenation callback', () => {
  const SHY = '\u00AD';
  /** Splits every word after its third letter. */
  const thirds = (word: string) => (word.length > 3 ? [word.slice(0, 3), word.slice(3)] : [word]);
  const texts = (r: ReturnType<typeof render>) => first<TextNode>(r).runs.map((x) => x.text);

  it('joins the parts of each word with soft hyphens, leaving the rest of the text alone', () => {
    const keep = (w: string) => (w === 'Inkoxide' ? [w] : thirds(w));
    const r = render(<Text hyphenationCallback={keep}>Inkoxide: typesetting, 2026 (régions) a b</Text>);
    expect(texts(r)).toEqual([`Inkoxide: typ${SHY}esetting, 2026 (rég${SHY}ions) a b`]);
  });

  it('leaves words that already have soft hyphens as written', () => {
    const r = render(<Text hyphenationCallback={thirds}>{`hyph${SHY}enation words`}</Text>);
    expect(texts(r)).toEqual([`hyph${SHY}enation wor${SHY}ds`]);
  });

  it('replaces the built-in patterns, unless a Text turns it off with null', () => {
    const r = render(
      <View>
        <Text hyphenate hyphenationCallback={thirds}>
          words
        </Text>
        <Text hyphenate hyphenationCallback={null}>
          words
        </Text>
      </View>,
    );
    const [a, b] = (first<BoxNode>(r).children ?? []) as TextNode[];
    expect(a?.style).toBeUndefined();
    expect(a?.runs[0]?.text).toBe(`wor${SHY}ds`);
    expect(b?.style).toEqual({ hyphenate: true });
    expect(b?.runs[0]?.text).toBe('words');
  });

  it('applies the registry callback to every Text, which a nested Text can override', () => {
    const reg = registry();
    reg.hyphenationCallback = thirds;
    const r = renderTemplate(
      <Document>
        <Page>
          <View>loose words</View>
          <Text>
            some <Text hyphenationCallback={(w) => [w]}>kept</Text> words
          </Text>
          <Text className="whitespace-nowrap">nowrap words</Text>
        </Page>
      </Document>,
      { fonts: reg },
    );
    const [view, text, nowrap] = r.document.pages[0].children as [BoxNode, TextNode, TextNode];
    const loose = view.children?.[0] as TextNode;
    expect(loose.runs[0]?.text).toBe(`loo${SHY}se wor${SHY}ds`);
    expect(text.runs[0]?.text).toBe(`som${SHY}e kept wor${SHY}ds`);
    expect(nowrap.runs[0]?.text).toBe('nowrap words');
  });

  it('is turned off by hyphenate={false} and hyphens-none, unless a Text gives its own', () => {
    const reg = registry();
    reg.hyphenationCallback = thirds;
    const r = renderTemplate(
      <Document>
        <Page>
          <Text hyphenate={false}>some words</Text>
          <Text className="hyphens-none">some words</Text>
          <Text>
            some <Text hyphenate={false}>words</Text> <Text className="hyphens-manual">words</Text>
          </Text>
          <Text className="hyphens-none">
            some <Text hyphenationCallback={thirds}>words</Text>
          </Text>
        </Page>
      </Document>,
      { fonts: reg },
    );
    const [off, none, inline, own] = r.document.pages[0].children as TextNode[];
    expect(off?.runs.map((x) => x.text)).toEqual(['some words']);
    expect(off?.style).toBeUndefined();
    expect(none?.runs.map((x) => x.text)).toEqual(['some words']);
    expect(inline?.runs.map((x) => x.text)).toEqual([`som${SHY}e words words`]);
    expect(own?.runs.map((x) => x.text)).toEqual([`some wor${SHY}ds`]);
  });

  it('keeps the patterns off a run with its own callback inside hyphenated text', () => {
    const r = render(
      <Text hyphenate>
        documentation <Text hyphenationCallback={(w) => [w]}>documentation</Text>{' '}
        <Text className="hyphens-none">documentation</Text>
      </Text>,
    );
    const text = first<TextNode>(r);
    expect(text.style).toEqual({ hyphenate: true });
    expect(text.runs.map(({ text, hyphenate }) => ({ text, hyphenate }))).toEqual([
      { text: 'documentation ', hyphenate: undefined },
      { text: 'documentation', hyphenate: false },
      { text: ' ', hyphenate: undefined },
      { text: 'documentation', hyphenate: false },
    ]);
  });

  it('never uses the patterns for a run in another language', () => {
    const r = render(
      <>
        <Text lang="en">
          hello{' '}
          <Text lang="de" hyphenate>
            Donaudampfschifffahrt
          </Text>{' '}
          <Text hyphenate>documentation</Text>
        </Text>
        <Text lang="en" hyphenate>
          documentation <Text lang="de">Donaudampfschifffahrt</Text>
        </Text>
      </>,
    );
    // Runs carry no language: the paragraph's English patterns would break the German word.
    const [on, off] = body(r) as TextNode[];
    expect(on?.runs.map(({ text, hyphenate }) => ({ text, hyphenate }))).toEqual([
      { text: 'hello Donaudampfschifffahrt ', hyphenate: undefined },
      { text: 'documentation', hyphenate: true },
    ]);
    expect(off?.style).toEqual({ hyphenate: true, lang: 'en' });
    expect(off?.runs.map(({ text, hyphenate }) => ({ text, hyphenate }))).toEqual([
      { text: 'documentation ', hyphenate: undefined },
      { text: 'Donaudampfschifffahrt', hyphenate: false },
    ]);
  });

  it('brings the patterns back for a run with a null callback', () => {
    const reg = registry();
    reg.hyphenationCallback = thirds;
    const r = renderTemplate(
      <Document lang="en-US">
        <Page>
          <Text hyphenate>
            some <Text hyphenationCallback={null}>words</Text>
          </Text>
          <Text>
            some <Text hyphenationCallback={null}>words</Text>
          </Text>
        </Page>
      </Document>,
      { fonts: reg },
    );
    const [on, off] = r.document.pages[0].children as TextNode[];
    // The callback replaces the patterns in the rest of the text.
    expect(on?.style).toBeUndefined();
    expect(on?.runs.map(({ text, hyphenate }) => ({ text, hyphenate }))).toEqual([
      { text: `som${SHY}e `, hyphenate: undefined },
      { text: 'words', hyphenate: true },
    ]);
    // Without `hyphenate`, a run without a callback is not hyphenated.
    expect(off?.runs.map(({ text, hyphenate }) => ({ text, hyphenate }))).toEqual([
      { text: `som${SHY}e words`, hyphenate: undefined },
    ]);
  });

  it('sees each word as displayed, after textTransform', () => {
    const asked: string[] = [];
    render(
      <Text
        className="uppercase"
        hyphenationCallback={(w) => {
          asked.push(w);
          return [w];
        }}
      >
        word
      </Text>,
    );
    expect(asked).toEqual(['WORD']);
  });

  it('asks once for each distinct word', () => {
    const asked: string[] = [];
    const r = render(
      <Text
        hyphenationCallback={(w) => {
          asked.push(w);
          return [w];
        }}
      >
        the cat and the hat and the bat
      </Text>,
    );
    expect(texts(r)).toEqual(['the cat and the hat and the bat']);
    expect(asked).toEqual(['the', 'cat', 'and', 'hat', 'bat']);
  });

  it('reports parts that do not make up the word, and exceptions, with the stack', () => {
    function Title() {
      return <Text hyphenationCallback={(w) => [w.toUpperCase()]}>word</Text>;
    }
    const bad = thrown(() => render(<Title />));
    expect(bad.kind).toBe('props');
    expect(bad.message).toContain('hyphenationCallback returned ["WORD"] for "word"');
    expect(bad.componentStack).toContain('in Title');
    const fails = thrown(() =>
      render(
        <Text
          hyphenationCallback={() => {
            throw new Error('no patterns');
          }}
        >
          word
        </Text>,
      ),
    );
    expect(fails.kind).toBe('render');
    expect(fails.message).toBe('hyphenationCallback threw for "word": no patterns');
  });
});

describe('fonts', () => {
  it('resolves family, weight and style to registered ids', () => {
    const r = render(
      <>
        <Text className="font-bold">a</Text>
        <Text className="italic">b</Text>
        <Text className="font-mono">c</Text>
      </>,
    );
    expect(body(r).map((n) => (n as TextNode).runs[0]?.font)).toEqual([
      'Inter-Bold',
      'Inter-Italic',
      'RobotoMono-Regular',
    ]);
    expect(r.fonts).toEqual(['Inter-Bold', 'Inter-Italic', 'NotoSans-Regular', 'RobotoMono-Regular']);
  });

  it('falls back to the nearest weight with a warning', () => {
    const r = render(<Text className="font-semibold">a</Text>);
    expect(first<TextNode>(r).runs[0]?.font).toBe('Inter-Bold');
    expect(r.warnings[0]).toBe('Font "Inter 600" is not registered; using Inter-Bold (Text in Page).');
  });

  it('declares only the fonts the document uses, plus the fallback font', () => {
    const r = render(<Text>a</Text>);
    expect(r.document.fonts?.map((f) => f.id)).toEqual(['Inter-Regular', 'NotoSans-Regular']);
    expect(r.document.fallbackFonts).toEqual(['NotoSans-Regular']);
  });

  it('follows CSS Fonts 4 weight matching', () => {
    const reg = new FontRegistry();
    for (const w of [100, 300, 400, 500, 700, 900]) reg.register({ id: `F${w}`, family: 'F', weight: w });
    const id = (w: number) => reg.resolve(['F'], w, 'normal').id;
    expect(id(450)).toBe('F500');
    expect(id(200)).toBe('F100');
    expect(id(600)).toBe('F700');
    expect(id(800)).toBe('F900');
    const sparse = new FontRegistry()
      .register({ id: 'L', family: 'F', weight: 300 })
      .register({ id: 'B', family: 'F', weight: 700 });
    expect(sparse.resolve(['F'], 400, 'normal').id).toBe('L');
    expect(sparse.resolve(['F'], 600, 'normal').id).toBe('B');
    expect(sparse.resolve(['Missing', 'F'], 700, 'italic')).toMatchObject({ id: 'B', exact: false });
  });
});

describe('tables', () => {
  it('maps columns, header rows, spans, row backgrounds and cell alignment', () => {
    const r = render(
      <Table columns={['2fr', '1fr', 60]} headerRows={1} className="w-full">
        <Row className="bg-neutral-100 font-bold">
          <Cell>Subject</Cell>
          <Cell className="text-right">Hours</Cell>
          <Cell className="text-right align-middle">Grade</Cell>
        </Row>
        <Row minHeight={20}>
          <Cell colSpan={2}>Total</Cell>
          <Cell verticalAlign="bottom">10</Cell>
        </Row>
      </Table>,
    );
    const t = first<TableNode>(r);
    expect(t.columns).toEqual([{ width: '2fr' }, { width: '1fr' }, { width: 60 }]);
    expect(t.headerRows).toBe(1);
    expect(t.style).toEqual({ width: '100%' });
    expect(t.rows[0]?.backgroundColor).toEqual([0.960587, 0.960587, 0.960587, 1]);
    expect(t.rows[0]?.cells[1]?.children?.[0]).toMatchObject({ type: 'text', style: { textAlign: 'right' } });
    expect(((t.rows[0]?.cells[0]?.children ?? [])[0] as TextNode).runs[0]?.font).toBe('Inter-Bold');
    expect(t.rows[0]?.cells[2]?.style).toEqual({ verticalAlign: 'middle' });
    expect(t.rows[1]).toMatchObject({ minHeight: 20 });
    expect(t.rows[1]?.cells[0]?.colSpan).toBe(2);
    expect(t.rows[1]?.cells[1]?.style).toEqual({ verticalAlign: 'bottom' });
  });

  it('rejects strings and foreign elements in a Table', () => {
    expect(thrown(() => render(<Table columns={[1]}>text</Table>)).message).toBe(
      'Text "text" cannot be directly inside <Table>; put it in a <Cell> of a <Row>.',
    );
    const inRow = () =>
      render(
        <Table columns={[1]}>
          <Row>Totals</Row>
        </Table>,
      );
    expect(thrown(inRow).message).toBe('Text "Totals" cannot be directly inside <Row>; put it in a <Cell>.');
    expect(
      thrown(() =>
        render(
          <Table columns={[1]}>
            <View />
          </Table>,
        ),
      ).message,
    ).toContain('<View> cannot be inside <Table>');
  });
});

describe('pages, images and structure', () => {
  it('maps the Document metadata props to meta', () => {
    const r = renderTemplate(
      <Document title="Report" creator="App" producer="Acme" pageMode="useThumbs" pageLayout="twoPageRight">
        <Page />
      </Document>,
      { fonts },
    );
    expect(r.document.meta).toEqual({
      title: 'Report',
      creator: 'App',
      producer: 'Acme',
      pageMode: 'useThumbs',
      pageLayout: 'twoPageRight',
    });
  });

  it('moves conformance out of the document, into the output options', () => {
    const output = (conformance?: unknown) =>
      renderTemplate(
        <Document title="A" conformance={conformance as 'PDF/A-2b'}>
          <Page />
        </Document>,
        { fonts },
      );
    expect(output().output).toEqual({});
    for (const [conformance, pdfA] of [
      ['PDF/A-1', '1b'],
      ['PDF/A-1b', '1b'],
      ['PDF/A-2', '2b'],
      ['PDF/A-2b', '2b'],
      ['PDF/A-3', '3b'],
      ['PDF/A-3b', '3b'],
    ]) {
      const r = output(conformance);
      expect(r.output).toEqual({ pdfA });
      expect(r.document.meta).toEqual({ title: 'A' });
      // Rebuilds for the page count keep it.
      expect(r.rebuild(2).output).toEqual({ pdfA });
    }
    for (const level of ['PDF/A-2a', 'PDF/A-3u', 'PDF/A-1a']) {
      expect(() => output(level)).toThrow(/the a and u levels need tagged PDF.*use 'PDF\/A-\db'/);
    }
    for (const bad of ['PDF/A-4', 'pdf/a-2b', 'PDF/UA-1', '2b', true]) {
      expect(() => output(bad)).toThrow(/is not a PDF\/A level: use 'PDF\/A-1b', 'PDF\/A-2b' or 'PDF\/A-3b'/);
    }
  });

  it('moves the passwords and permissions into output options that never show them', () => {
    const secret = 's3cret-marker';
    const r = renderTemplate(
      <Document
        title="A"
        userPassword={secret}
        ownerPassword={`${secret}-owner`}
        permissions={{ copying: true }}
      >
        <Page />
      </Document>,
      { fonts },
    );
    expect(r.output.encryption).toEqual({
      userPassword: secret,
      ownerPassword: `${secret}-owner`,
      permissions: { copying: true },
    });
    expect(r.document.meta).toEqual({ title: 'A' });
    expect(JSON.stringify(r.document)).not.toContain(secret);
    for (const shown of [JSON.stringify(r), JSON.stringify(r.output), inspect(r), inspect(r.output)]) {
      expect(shown).not.toContain(secret);
    }
    expect(JSON.parse(JSON.stringify(r.output))).toEqual({ encryption: '<redacted>' });
    expect(r.rebuild(2).output.encryption?.userPassword).toBe(secret);
    // Without the props there is no encryption.
    const plain = renderTemplate(
      <Document>
        <Page />
      </Document>,
      { fonts },
    );
    expect(plain.output.encryption).toBeUndefined();
    // Wrong types fail without showing the value.
    for (const props of [{ userPassword: 12345 }, { ownerPassword: [secret] }, { permissions: secret }]) {
      const e = thrown(() =>
        renderTemplate(
          <Document {...(props as object)}>
            <Page />
          </Document>,
          { fonts },
        ),
      );
      expect(e.message).toMatch(/<Document (userPassword|ownerPassword|permissions)> must be/);
      expect(e.message).not.toContain(secret);
    }
  });

  it('maps page size presets, margins, background, header and footer', () => {
    const r = renderTemplate(
      <Document>
        <Page size="A4-landscape" margin={20} className="bg-white">
          <Header>
            <Text>h</Text>
          </Header>
          <Footer>
            <Text>f</Text>
          </Footer>
          <Spacer h={10} />
        </Page>
      </Document>,
      { fonts },
    );
    const p = r.document.pages[0];
    expect(p.size).toEqual({ width: 841.89, height: 595.28 });
    expect(p.margin).toEqual({ top: 20, right: 20, bottom: 20, left: 20 });
    expect(p.background).toEqual([1, 1, 1, 1]);
    expect(p.header?.type).toBe('box');
    expect(p.footer?.type).toBe('box');
    expect(p.children).toEqual([{ type: 'spacer', style: { height: 10 } }]);
  });

  it('reads percentage padding on a page as a share of the page width, on every side', () => {
    const page = (props: { className?: string; style?: object }) =>
      renderTemplate(
        <Document>
          <Page size={[400, 600]} {...props}>
            <Header>
              <Text>h</Text>
            </Header>
            <Text>x</Text>
          </Page>
        </Document>,
        { fonts },
      ).document.pages[0];
    const all = { top: 40, right: 40, bottom: 40, left: 40 };
    expect(page({ style: { padding: '10%' } }).margin).toEqual(all);
    expect(page({ className: 'p-[10%]' }).margin).toEqual(all);
    const p = page({ style: { paddingTop: '5%', paddingLeft: 12 } });
    expect(p.margin).toEqual({ top: 20, left: 12 });
    expect((p.header as BoxNode | undefined)?.style).toEqual({
      padding: { left: 12 },
      minHeight: 20,
      justifyContent: 'center',
    });
  });

  it('rejects a percentage minimum height on a Row, which has no table height to resolve against', () => {
    const table = (props: { className?: string; style?: object }) => () =>
      render(
        <Table columns={[1]}>
          <Row {...props}>
            <Cell />
          </Row>
        </Table>,
      );
    const cls = thrown(table({ className: 'min-h-[10%]' }));
    expect([cls.kind, cls.className]).toEqual(['tailwind', 'min-h-[10%]']);
    expect(cls.message).toBe(
      '<Row>: a minimum height of 10% (from className="min-h-[10%]") has no table height to be a percentage of; use a length.',
    );
    const style = thrown(table({ style: { minHeight: '10%' } }));
    expect([style.kind, style.message]).toEqual([
      'props',
      '<Row>: a minimum height of 10% (from its style) has no table height to be a percentage of; use a length.',
    ]);
    const ok = (props: { className?: string; style?: object }) =>
      (body(table(props)() as ReturnType<typeof render>)[0] as TableNode).rows[0]?.minHeight;
    expect(ok({ style: { minHeight: '1cm' } })).toBe(28.346457);
    expect(ok({ style: { minHeight: 'auto' } })).toBeUndefined();
  });

  it('lines header and footer up with the page margins unless they say otherwise', () => {
    const r = renderTemplate(
      <Document>
        <Page margin={{ top: 40, right: 30, bottom: 50, left: 20 }}>
          <Header>
            <Text>h</Text>
          </Header>
          <Footer className="px-[10pt] pb-[8pt]">
            <Text>f</Text>
          </Footer>
        </Page>
      </Document>,
      { fonts },
    );
    const p = r.document.pages[0];
    expect((p.header as BoxNode).style).toEqual({
      padding: { right: 30, left: 20 },
      minHeight: 40,
      justifyContent: 'center',
    });
    // Its own padding wins, and a vertical inset turns off the centring.
    expect((p.footer as BoxNode).style).toEqual({ padding: { right: 10, bottom: 8, left: 10 } });
  });

  it('centres a row header with align-items, so justify-between still spreads it out', () => {
    const r = renderTemplate(
      <Document>
        <Page margin={40}>
          <Header className="flex-row justify-between">
            <Text>left</Text>
            <Text>right</Text>
          </Header>
          <Footer className="flex-row items-end">
            <Text>f</Text>
          </Footer>
        </Page>
      </Document>,
      { fonts },
    );
    const p = r.document.pages[0];
    expect((p.header as BoxNode).style).toMatchObject({
      flexDirection: 'row',
      justifyContent: 'space-between',
      alignItems: 'center',
      minHeight: 40,
    });
    // Its own vertical alignment turns the centring off.
    expect((p.footer as BoxNode).style).not.toHaveProperty('minHeight');
  });

  it('passes image bytes through the render options and rejects remote URLs', () => {
    const bytes = new Uint8Array([0xff, 0xd8, 0xff]);
    const r = render(
      <>
        <Image src={bytes} fit="contain" className="w-20" />
        <Image src={bytes} />
        <Image src="logo" />
      </>,
    );
    expect(body(r)).toEqual([
      { type: 'image', style: { width: 60 }, src: 'image-0', fit: 'contain' },
      { type: 'image', src: 'image-0' },
      { type: 'image', src: 'logo' },
    ]);
    expect(r.images).toEqual({ 'image-0': bytes });
    expect(thrown(() => render(<Image src="https://example.com/a.png" />)).message).toContain(
      'remoteImages allowlist',
    );
  });

  it('says where a string outside a Text-like element belongs', () => {
    const doc = () => renderTemplate(<Document>Hello</Document>, { fonts });
    expect(thrown(doc).message).toBe(
      'Text "Hello" cannot be directly inside <Document>; put it in a <Page>.',
    );
    // @ts-expect-error: an Image has no children
    const image = () => render(<Image src="logo">x</Image>);
    expect(thrown(image).message).toBe('Text "x": <Image> cannot have children.');
    expect(thrown(() => render(<section>x</section>)).message).toContain(
      '<section> is not an Inkoxide component',
    );
  });

  it('rejects a View inside Text, HTML elements and two headers', () => {
    expect(
      thrown(() =>
        render(
          <Text>
            <View />
          </Text>,
        ),
      ).message,
    ).toContain('<View> cannot be inside <Text>');
    expect(thrown(() => render(<div />)).message).toContain('<div> is not an Inkoxide component');
    const two = () =>
      render(
        <>
          <Header />
          <Header />
        </>,
      );
    expect(thrown(two).message).toContain('only one <Header>');
  });
});

describe('errors carry component stacks', () => {
  it('reports unsupported classes with the class string and the stack', () => {
    function Card() {
      return <View className="p-2 shadow-lg" />;
    }
    const e = thrown(() => render(<Card />));
    expect(e.kind).toBe('tailwind');
    expect(e.className).toBe('p-2 shadow-lg');
    expect(e.message).toContain('box-shadow');
    expect(e.componentStack).toContain('in View');
    expect(e.componentStack).toContain('in Card');
  });

  it('reports exceptions thrown by template code', () => {
    function Broken(): ReactNode {
      throw new Error('data.child is undefined');
    }
    const e = thrown(() => render(<Broken />));
    expect(e.kind).toBe('render');
    expect(e.message).toContain('data.child is undefined');
    expect(e.componentStack).toContain('in Broken');
  });

  it('reports a component that suspends, with or without a Suspense boundary', async () => {
    const settled = Promise.resolve('late');
    function Late({ p }: { p: Promise<string> }) {
      return <Text>{use(p)}</Text>;
    }
    function Card({ p }: { p: Promise<string> }) {
      return (
        <View>
          <Text>before</Text>
          <Late p={p} />
        </View>
      );
    }
    const bare = thrown(() => render(<Card p={settled} />));
    expect(bare.kind).toBe('render');
    expect(bare.message).toMatch(/^Late suspended\. Suspense, use\(\) with a promise/);
    expect(bare.componentStack).toBe('\n    in Late\n    in View\n    in Card\n    in Page\n    in Document');
    const LazyCard = lazy(async () => ({ default: Card }));
    expect(thrown(() => render(<LazyCard p={settled} />)).message).toMatch(/^A lazy component suspended\. /);
    const Chart = Object.assign(
      lazy(() => new Promise<{ default: typeof Card }>(() => {})),
      { displayName: 'Chart' },
    );
    const lazyChart = thrown(() => render(<Chart p={settled} />));
    expect(lazyChart.message).toMatch(/^Chart suspended\. /);
    expect(lazyChart.componentStack).toBe('\n    in Chart\n    in Page\n    in Document');
    const renderProp = thrown(() =>
      render(
        <View>
          <Text render={() => use(settled)} />
        </View>,
      ),
    );
    expect(renderProp.message).toMatch(/^The render prop of <Text> suspended\. /);
    expect(thrown(() => render(<View render={() => <Late p={settled} />} />)).message).toMatch(
      /^Late suspended\. /,
    );

    const pending = new Promise<string>(() => {});
    const inBoundary = thrown(() =>
      render(
        <Suspense fallback={<Text>loading</Text>}>
          <Card p={pending} />
        </Suspense>,
      ),
    );
    expect(inBoundary.kind).toBe('render');
    expect(inBoundary.message).toContain('A component inside <Suspense> suspended');
    expect(inBoundary.componentStack).toBe('\n    in Suspense\n    in Page\n    in Document');
    // React retries the boundary later, from a timer: that must not crash the process.
    await new Promise((ok) => setTimeout(ok, 500));
  });

  it('rejects layout utilities on inline Text', () => {
    const e = thrown(() =>
      render(
        <Text>
          a <Text className="p-2">b</Text>
        </Text>,
      ),
    );
    expect(e.message).toContain('nested <Text> is an inline run');
    const link = thrown(() =>
      render(
        <Text>
          a{' '}
          <Link href="#a" className="p-2">
            b
          </Link>
        </Text>,
      ),
    );
    expect(link.message).toContain('A <Link> inside <Text> is an inline run');
  });

  it('rejects truncate and line clamping on a Page, Row or Spacer, as on other boxes', () => {
    const page = (props: { className?: string; style?: object }) => () =>
      renderTemplate(
        <Document>
          <Page {...props}>
            <Text>x</Text>
          </Page>
        </Document>,
        { fonts },
      );
    expect(thrown(page({ className: 'truncate' })).message).toBe(
      'truncate and line-clamp-* apply to <Text>, not <Page> (className="truncate").',
    );
    expect(thrown(page({ style: { maxLines: 2, ellipsis: true } })).message).toBe(
      'maxLines and ellipsis apply to <Text>, not <Page> (from its style).',
    );
    const row = () =>
      render(
        <Table columns={[1]}>
          <Row style={{ maxLines: 1 }}>
            <Cell />
          </Row>
        </Table>,
      );
    expect(thrown(row).message).toBe('maxLines applies to <Text>, not <Row> (from its style).');
    const spacer = () => render(<Spacer style={{ maxLines: 1 }} />);
    expect(thrown(spacer).message).toBe('maxLines applies to <Text>, not <Spacer> (from its style).');
  });

  it('rejects the props of a paragraph on an inline Text or Link', () => {
    const inline =
      (props: Record<string, unknown>, link = false) =>
      () =>
        render(
          <Text>
            a{' '}
            {link ? (
              <Link href="#a" {...props}>
                b
              </Link>
            ) : (
              <Text {...props}>b</Text>
            )}
          </Text>,
        );
    const cases: Record<string, unknown>[] = [
      { id: 'terms' },
      { bookmark: 'Terms' },
      { pages: 'first' },
      { render: () => 'x' },
      { dir: 'rtl' },
      { orphans: 2 },
      { widows: 2 },
      { break: 'page-before' },
      { wrap: false },
      { minPresenceAhead: 20 },
    ];
    for (const props of cases) {
      const [key] = Object.keys(props);
      const e = thrown(inline(props));
      expect(e.kind).toBe('props');
      expect(e.message).toBe(
        `A nested <Text> is an inline run of the paragraph, so \`${key}\` has nothing to apply to. Put it on the outer <Text>, or make this text a <Text> of its own.`,
      );
      expect(e.componentStack).toMatch(/^\n {4}in Text\n {4}in Page/);
    }
    expect(thrown(inline({ id: 'x' }, true)).message).toMatch(/^A <Link> inside <Text> is an inline run/);
    // Defaults, `debug`, and the props of a run are fine.
    const ok = render(
      <Text>
        a{' '}
        <Text wrap break={false} debug lang="de" href="#a">
          b
        </Text>
      </Text>,
    );
    expect(first<TextNode>(ok).runs.map((r) => r.link)).toEqual([undefined, '#a']);
  });

  it('blames a style key the element does not take on its style, and a class on its className', () => {
    const style = thrown(() => render(<View style={{ maxLines: 2 }} />));
    expect([style.kind, style.className, style.message]).toEqual([
      'props',
      undefined,
      'maxLines applies to <Text>, not <View> (from its style).',
    ]);
    const cls = thrown(() => render(<View className="truncate" style={{ padding: 2 }} />));
    expect([cls.kind, cls.className]).toEqual(['tailwind', 'truncate']);
    expect(cls.message).toBe('truncate and line-clamp-* apply to <Text>, not <View> (className="truncate").');

    const row = (props: { className?: string; style?: object }) => () =>
      render(
        <Table columns={[1]}>
          <Row {...props}>
            <Cell />
          </Row>
        </Table>,
      );
    const rowStyle = thrown(row({ className: 'bg-white', style: { padding: 4 } }));
    expect([rowStyle.kind, rowStyle.className]).toEqual(['props', undefined]);
    expect(rowStyle.message).toBe(
      '<Row> does not support "paddingTop" (from its style). A Row takes a background, a minimum height and text styles; style the cells instead.',
    );
    const rowClass = thrown(row({ className: 'p-1' }));
    expect([rowClass.kind, rowClass.className]).toEqual(['tailwind', 'p-1']);
    expect(rowClass.message).toContain('(from className="p-1")');

    const inline = thrown(() =>
      render(
        <Text>
          a <Text style={{ padding: 4 }}>b</Text>
        </Text>,
      ),
    );
    expect([inline.kind, inline.className]).toEqual(['props', undefined]);
    expect(inline.message).toContain('only text styles apply, not "paddingTop, paddingRight');
    expect(inline.message).toContain('(from its style)');
  });

  it('rejects a Link without a target and block content inside an inline Link', () => {
    // @ts-expect-error: a Link needs href or src
    expect(thrown(() => render(<Link>x</Link>)).message).toContain('<Link> needs an href');
    // @ts-expect-error: a Link needs href or src
    expect(thrown(() => render(<Link className="p-2">x</Link>)).message).toContain('<Link> needs an href');
    const inline = () =>
      render(
        <Text>
          <Link href="#a">
            <View />
          </Link>
        </Text>,
      );
    expect(thrown(inline).message).toContain('<View> cannot be inside <Text>');
  });

  it('maps engine and ajv paths back to components', () => {
    function Field({ label }: { label: string }) {
      return (
        <View>
          <Text>{label}</Text>
        </View>
      );
    }
    const r = render(
      <>
        <Text>intro</Text>
        <Field label="Name" />
      </>,
    );
    const at = r.locate('pages[0].children[1].children[0].runs[0].font');
    expect(at?.component).toBe('Text');
    expect(at?.componentStack).toContain('in Field');
  });

  it('validates the tree with ajv and maps problems to components', () => {
    function Bad() {
      return <View style={{ opacity: 3 }} />;
    }
    const r = render(<Bad />);
    const [problem] = r.validate();
    expect(problem?.path).toBe('pages[0].children[0].style.opacity');
    expect(problem?.componentStack).toContain('in Bad');
  });
});

describe('renderer', () => {
  it('reports the version of the package', () => {
    const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
    expect(hostConfig.rendererVersion).toBe(pkg.version);
  });
});
