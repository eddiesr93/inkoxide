import type { BoxNode, TableNode, TextNode } from '@inkoxide/schema';
import { createContext, type ReactNode, useContext } from 'react';
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
  TemplateError,
  Text,
  TotalPages,
  View,
} from '../src/index.ts';
import { registry, setup } from './helpers.ts';

beforeAll(setup);

const fonts = registry();
const render = (children: ReactNode, pageClass = 'text-[10pt]') =>
  renderTemplate(
    <Document lang="en-US">
      <Page className={pageClass}>{children}</Page>
    </Document>,
    { fonts },
  );
const body = (r: ReturnType<typeof render>) => r.document.pages[0].children ?? [];
const first = <T,>(r: ReturnType<typeof render>) => body(r)[0] as T;

function thrown(fn: () => unknown): TemplateError {
  try {
    fn();
  } catch (e) {
    if (e instanceof TemplateError) return e;
    throw e;
  }
  throw new Error('expected a TemplateError');
}

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
    expect(r.warnings[0]).toContain('Inter 600');
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
    expect(thrown(() => render(<Table columns={[1]}>text</Table>)).message).toContain(
      'cannot be directly inside <Table>',
    );
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

  it('rejects a Link without a target and block content inside an inline Link', () => {
    expect(thrown(() => render(<Link>x</Link>)).message).toContain('<Link> needs an href');
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
