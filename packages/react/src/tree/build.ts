import type {
  Bookmark,
  Break,
  Color,
  ColumnSpec,
  Document,
  FieldNode,
  ImageNode,
  Meta,
  PageFilter,
  PageSpec,
  Node as SchemaNode,
  Style,
  TableCell,
  TableNode,
  TableRow,
  TextNode,
  TextRun,
} from '@inkoxide/schema';
import { round, type TwStyle } from '@inkoxide/style';
import { createElement } from 'react';

import { Painter } from '../canvas.ts';
import {
  type AbsoluteLength,
  type BookmarkProp,
  type BreakProp,
  type FieldProps,
  HOST,
  HOST_NAME,
  type HyphenationCallback,
  type Length,
  type PageSize,
  type RenderProp,
  type Style as StyleInput,
} from '../components/index.ts';
import { TemplateError } from '../errors.ts';
import type { FontRegistry } from '../fonts/registry.ts';
import { PageContext, RenderCall } from '../page.ts';
import {
  type Container,
  componentNames,
  componentStack,
  type Instance,
  type Node,
} from '../reconciler/instance.ts';
import { renderToInstances } from '../reconciler/render.ts';
import { classStyle, defaultFamily, isUnsupportedClass } from '../tailwind.ts';
import { ImageIds, ImageTable, type PendingImage, type RemoteImagePolicy } from './images.ts';
import { PRESET_NAMES, presetSize } from './page-sizes.ts';
import {
  boxKeys,
  boxStyle,
  callbackOf,
  hasInherited,
  inherit,
  type TextCtx,
  textNodeStyle,
  textOnlyKeys,
  toPt,
  type Units,
  usesPatterns,
  withStyleProp,
} from './style.ts';
import { svgDataUrl, svgDocument, svgMarkup } from './svg.ts';
import { Hyphenator, transformText } from './text.ts';
import { stackVariants } from './variants.ts';

export interface BuildOptions {
  fonts: FontRegistry;
  /** Default font stack (Tailwind's `--font-sans`). */
  defaultFamily?: readonly string[];
  /** Record which instance produced each output object (for error paths). */
  track?: boolean;
  /** Page count from a previous render, for `render` props (1 when unknown). */
  pageCount?: number;
  remoteImages?: RemoteImagePolicy;
  /** File paths in `src` are images to load (the caller reads them); otherwise they are ids. */
  localImages?: boolean;
  /** Splits words into syllables for every `Text` without its own `hyphenationCallback`. */
  hyphenationCallback?: HyphenationCallback | undefined;
  /** Ids of the images the caller passes in: a `src` string naming one is that image, path or not. */
  imageIds?: readonly string[];
  /** The id table of the tree this build rebuilds; a new one when omitted. */
  generatedIds?: ImageIds;
}

export interface BuildResult {
  document: Document;
  /** Image bytes referenced by generated `src` ids; pass them as the render option `images`. */
  images: Record<string, Uint8Array>;
  /** Font ids used, in registration order. */
  fonts: string[];
  /** Non-fatal problems (font fallbacks). */
  warnings: string[];
  /** Output object → the instance that produced it (only with `track`). */
  origin: Map<object, Instance> | undefined;
  /** True when a `render` prop was used: the tree depends on the page count. */
  dynamic: boolean;
  /** Remote images to fetch: image id → URL (allowed by `remoteImages`). */
  remote: Record<string, string>;
  /** Other images to load: image id → a file, a request, or a function or promise `src`. */
  pending: Record<string, PendingImage>;
}

type Inline = Instance | { kind: 'text'; text: string; fiber: unknown };

const isEl = (n: Node | undefined, type: string): n is Instance => n?.kind === 'element' && n.type === type;

function sameColor(a: Color | undefined, b: Color | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3];
}

function toBreak(b: BreakProp | undefined): Break | undefined {
  if (b === undefined) return undefined;
  if (b === 'avoid') return { inside: 'avoid' };
  if (b === 'page-before') return { before: 'page' };
  if (b === 'page-after') return { after: 'page' };
  return b;
}

/** Whether the element has the `fixed` prop. */
function isFixed(inst: Instance): boolean {
  return inst.props.fixed !== undefined && inst.props.fixed !== false;
}

/** Whether `b` sets a pagination control: a break before, after or inside, or `minPresenceAhead`. */
function paginates(b: Break | undefined): boolean {
  if (b === undefined) return false;
  const set = (v: string | undefined) => v !== undefined && v !== 'auto';
  return set(b.before) || set(b.after) || set(b.inside) || b.minPresenceAhead !== undefined;
}

function toBookmark(b: BookmarkProp | undefined): Bookmark | undefined {
  if (b === undefined) return undefined;
  return typeof b === 'string' ? { title: b } : b;
}

class Builder {
  readonly imageTable: ImageTable;
  private readonly used = new Set<string>();
  readonly warnings: string[] = [];
  private readonly warned = new Set<string>();
  readonly origin: Map<object, Instance> | undefined;
  dynamic = false;
  private docLang: string | undefined;
  /** `debug` on the `<Document>`: every page outlines its elements. */
  private debugAll = false;
  private readonly hyphenator = new Hyphenator();
  /** The size of the page being built: what `vw` and `vh` are relative to. */
  private pageBox = { width: 0, height: 0 };
  /** Elements placed directly in a `<Page>`: the only ones that can be `fixed`. */
  private readonly onPage = new WeakSet<Instance>();
  /** The band being built (`'Header'` or `'Footer'`), if any. */
  private band: string | undefined;
  /** The fixed element being built (`'View'`, `'Text'`, …), if any. */
  private fixedEl: string | undefined;
  /** How many table cells the element being built is in. */
  private inCell = 0;

  constructor(private readonly opts: BuildOptions) {
    this.origin = opts.track ? new Map() : undefined;
    const registered = new Set(opts.imageIds);
    this.imageTable = new ImageTable({
      registered,
      generated: opts.generatedIds ?? new ImageIds(registered),
      remoteImages: opts.remoteImages,
      localImages: opts.localImages,
    });
  }

  private fail(
    inst: Instance | undefined,
    message: string,
    init: ConstructorParameters<typeof TemplateError>[1] = {},
  ): never {
    throw new TemplateError(message, { componentStack: componentStack(inst), ...init });
  }

  private mark<T extends object>(out: T, inst: Instance): T {
    this.origin?.set(out, inst);
    return out;
  }

  /** The element's own style: its classes, then its `style` prop. `parent` is the inherited context. */
  private styleOf(inst: Instance, parent: TextCtx): TwStyle {
    const className = inst.props.className as string | undefined;
    let s: TwStyle = {};
    if (className?.trim()) {
      let classes: TwStyle | string;
      try {
        classes = classStyle(className);
      } catch (e) {
        if (isUnsupportedClass(e)) {
          this.fail(inst, `${e.message}. In className="${className}" on <${HOST_NAME[inst.type]}>.`, {
            kind: 'tailwind',
            className,
            cause: e,
          });
        }
        throw e;
      }
      if (typeof classes === 'string') {
        this.fail(inst, `className="${className}" on <${HOST_NAME[inst.type]}> needs Tailwind: ${classes}.`, {
          kind: 'tailwind',
          className,
        });
      }
      s = classes;
    }
    const style = inst.props.style as StyleInput | undefined;
    if (style) {
      try {
        s = withStyleProp(s, style, parent.fontSize, this.pageBox);
      } catch (e) {
        this.fail(inst, `Invalid style prop on <${HOST_NAME[inst.type]}>: ${(e as Error).message}`, {
          kind: 'props',
          cause: e,
        });
      }
    }
    if ((s.position as string | undefined) === 'fixed') {
      this.fail(
        inst,
        `<${HOST_NAME[inst.type]}>: position: 'fixed' is not supported; to repeat an element on every page, give it the fixed prop and position: 'absolute'.`,
        { kind: 'props' },
      );
    }
    if (inst.type !== HOST.Image) {
      for (const key of ['objectFit', 'objectPosition'] as const) {
        if (s[key] !== undefined) {
          this.fail(inst, `${key} applies to <Image>, not <${HOST_NAME[inst.type]}>.`, { kind: 'props' });
        }
      }
    }
    return s;
  }

  private font(ctx: TextCtx, inst: Instance | undefined): string {
    if (ctx.fontId !== undefined) return ctx.fontId;
    const r = this.opts.fonts.resolve(ctx.fontFamily, ctx.fontWeight, ctx.fontStyle);
    ctx.fontId = r.id;
    if (!r.exact && r.wanted && !this.warned.has(r.wanted)) {
      this.warned.add(r.wanted);
      const parent = inst ? componentNames(inst)[1] : undefined;
      const where = inst ? ` (${HOST_NAME[inst.type]}${parent === undefined ? '' : ` in ${parent}`})` : '';
      this.warnings.push(`Font "${r.wanted}" is not registered; using ${r.id}${where}.`);
    }
    this.used.add(r.id);
    return r.id;
  }

  private onlyKeys(inst: Instance, s: TwStyle, allowed: ReadonlySet<string>, hint: string): void {
    for (const k of boxKeys(s)) {
      if (!allowed.has(k)) {
        this.fail(
          inst,
          `<${HOST_NAME[inst.type]}> does not support "${k}" (from ${inst.props.style ? 'its style or ' : ''}className="${inst.props.className ?? ''}"). ${hint}`,
          {
            kind: 'tailwind',
            className: inst.props.className as string | undefined,
          },
        );
      }
    }
  }

  private noTextOnly(inst: Instance, s: TwStyle): void {
    const keys = textOnlyKeys(s);
    if (keys.length > 0) {
      this.fail(inst, `truncate and line-clamp-* apply to <Text>, not <${HOST_NAME[inst.type]}>.`, {
        kind: 'tailwind',
        className: inst.props.className as string | undefined,
      });
    }
  }

  build(container: Container): Document {
    const [root, ...extra] = container.children.filter((c) => !(c.kind === 'text' && c.text.trim() === ''));
    if (!isEl(root, HOST.Document) || extra.length > 0) {
      throw new TemplateError('A template must render exactly one <Document> at the top.');
    }
    const p = root.props;
    const meta: Meta = {};
    for (const k of ['title', 'author', 'subject', 'keywords', 'creator', 'lang', 'creationDate'] as const) {
      const v = p[k];
      if (typeof v === 'string') meta[k] = v;
    }
    this.docLang = meta.lang;
    this.debugAll = p.debug === true;
    const family = this.opts.defaultFamily ?? defaultFamily();
    const ctx: TextCtx = {
      fontFamily: family,
      fontWeight: 400,
      fontStyle: 'normal',
      fontSize: 12,
      color: [0, 0, 0, 1],
      lang: meta.lang,
      hyphenation: this.opts.hyphenationCallback,
      link: undefined,
    };
    const pages = root.children
      .filter((c): c is Instance => c.kind === 'element')
      .map((pg) => this.page(pg, ctx));
    if (pages.length === 0) this.fail(root, '<Document> needs at least one <Page>.');

    const fallback = (p.fallbackFonts as string[] | undefined) ?? ['NotoSans-Regular'];
    const fallbackFonts = fallback.filter((id) => this.opts.fonts.has(id));
    for (const id of fallbackFonts) this.used.add(id);
    const fontIds = this.fontIds();
    const doc: Document = {
      version: 1,
      ...(Object.keys(meta).length > 0 ? { meta } : {}),
      fonts: fontIds.map((id) => this.opts.fonts.decl(id)),
      ...(fallbackFonts.length > 0 ? { fallbackFonts } : {}),
      pages: pages as [PageSpec, ...PageSpec[]],
    };
    return this.mark(doc, root);
  }

  fontIds(): string[] {
    return this.opts.fonts.ids().filter((id) => this.used.has(id));
  }

  private page(inst: Instance, parent: TextCtx): PageSpec {
    let size = this.pageSize(inst, (inst.props.size as PageSize | undefined) ?? 'A4');
    const orientation = inst.props.orientation as 'portrait' | 'landscape' | undefined;
    if (
      (orientation === 'landscape' && size.height > size.width) ||
      (orientation === 'portrait' && size.width > size.height)
    ) {
      size = { width: size.height, height: size.width };
    }
    this.pageBox = size;
    const own = this.styleOf(inst, parent);
    // Padding on a Page is its margin (`<Page style={{ padding: 48 }}>`).
    this.onlyKeys(
      inst,
      own,
      new Set(['backgroundColor', 'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft']),
      'A Page takes text styles, a background and padding (its margin).',
    );
    const ctx = inherit(parent, own);
    const units = this.units(own, parent);
    const spec: PageSpec = { size };
    const m = inst.props.margin as
      | Length
      | Partial<Record<'top' | 'right' | 'bottom' | 'left', Length>>
      | undefined;
    if (typeof m === 'number' || typeof m === 'string') {
      const v = this.length(inst, m, units);
      spec.margin = { top: v, right: v, bottom: v, left: v };
    } else if (m) {
      spec.margin = {};
      for (const k of ['top', 'right', 'bottom', 'left'] as const) {
        const v = m[k];
        if (v !== undefined) spec.margin[k] = this.length(inst, v, units);
      }
    }
    // Padding fills the sides the margin prop leaves unset.
    const pad = {
      top: own.paddingTop,
      right: own.paddingRight,
      bottom: own.paddingBottom,
      left: own.paddingLeft,
    };
    for (const k of ['top', 'right', 'bottom', 'left'] as const) {
      const v = pad[k];
      if (typeof v === 'number' && spec.margin?.[k] === undefined) spec.margin = { ...spec.margin, [k]: v };
    }
    if (own.backgroundColor !== undefined) {
      spec.background = own.backgroundColor === 'currentColor' ? ctx.color : own.backgroundColor;
    }
    if (this.debugAll || inst.props.debug === true) spec.debug = true;
    const flow: Node[] = [];
    for (const child of inst.children) {
      if (isEl(child, HOST.Header) || isEl(child, HOST.Footer)) {
        const key = child.type === HOST.Header ? 'header' : 'footer';
        if (spec[key]) this.fail(child, `A <Page> can have only one <${HOST_NAME[child.type]}>.`);
        this.band = HOST_NAME[child.type];
        try {
          spec[key] = this.box(
            child,
            ctx,
            child.children,
            bandDefaults(spec.margin, key, this.styleOf(child, ctx)),
          );
        } finally {
          this.band = undefined;
        }
      } else {
        flow.push(child);
      }
    }
    for (const child of flow) if (child.kind === 'element') this.onPage.add(child);
    const children = this.children(flow, ctx, inst);
    if (children.length > 0) spec.children = children;
    return this.mark(spec, inst);
  }

  /**
   * A page `size` in points: a preset (`'A4'`, `'letter-landscape'`), `[width, height]`,
   * `[width]` or a bare length (both square), or `{ width, height }`.
   */
  private pageSize(inst: Instance, size: PageSize): { width: number; height: number } {
    if (Array.isArray(size)) {
      const [w, h, ...extra] = size as AbsoluteLength[];
      if (w === undefined || extra.length > 0) {
        this.fail(inst, `<Page size>: use [width, height] or [width], not ${size.length} values.`, {
          kind: 'props',
        });
      }
      const width = this.length(inst, w);
      return { width, height: h === undefined ? width : this.length(inst, h) };
    }
    if (typeof size === 'number') {
      const side = this.length(inst, size);
      return { width: side, height: side };
    }
    if (typeof size === 'string') {
      const preset = presetSize(size);
      if (preset) return preset;
      let side: unknown;
      try {
        side = toPt(size);
      } catch {
        // Not a length either: unknown name.
      }
      if (typeof side === 'number') return { width: side, height: side };
      this.fail(
        inst,
        `Unknown page size "${size}". Use a preset (${PRESET_NAMES}), optionally with "-landscape", a length, [width, height] or { width, height }.`,
        { kind: 'props' },
      );
    }
    return { width: this.length(inst, size.width), height: this.length(inst, size.height) };
  }

  /** Block children; runs of bare strings become one anonymous text node. */
  private children(nodes: Node[], ctx: TextCtx, parent: Instance): SchemaNode[] {
    const out: SchemaNode[] = [];
    let pending: Inline[] | undefined;
    const flush = () => {
      if (!pending) return;
      if (pending.some((n) => n.kind !== 'text' || n.text.trim() !== '')) {
        const node: TextNode = { type: 'text', runs: [] };
        this.runs(pending, ctx, node.runs, parent);
        const style = textNodeStyle(undefined, {}, ctx, this.docLang);
        if (style) node.style = style;
        out.push(this.mark(node, parent));
      }
      pending = undefined;
    };
    for (const n of nodes) {
      if (n.kind === 'text') {
        pending ??= [];
        pending.push(n);
        continue;
      }
      flush();
      out.push(this.blockNode(n, ctx));
    }
    flush();
    return out;
  }

  /** What `em`, `vw` and `vh` in an element's length props are relative to. */
  private units(own: TwStyle, parent: TextCtx): Units {
    return { em: own.fontSize ?? parent.fontSize, page: this.pageBox };
  }

  /** Points from a length prop (`12`, `'20mm'`); `em`, `vw` and `vh` need `units`. */
  private length(inst: Instance, v: Length, units?: Units): number {
    let out: unknown;
    try {
      out = toPt(v, units);
    } catch (e) {
      this.fail(inst, `<${HOST_NAME[inst.type]}>: ${(e as Error).message}.`, { kind: 'props' });
    }
    if (typeof out !== 'number') {
      this.fail(inst, `<${HOST_NAME[inst.type]}>: "${String(v)}" must be an absolute length.`, {
        kind: 'props',
      });
    }
    return out;
  }

  private flowProps(
    node: {
      id?: string;
      break?: Break;
      link?: string;
      bookmark?: Bookmark;
      pages?: PageFilter;
      style?: Style;
      debug?: boolean;
    },
    inst: Instance,
  ): void {
    const p = inst.props;
    if (typeof p.id === 'string') node.id = p.id;
    if (p.pages !== undefined) node.pages = p.pages as PageFilter;
    if (p.debug === true) node.debug = true;
    // Aliases for the same behaviour: `wrap`, `break={true}` and `minPresenceAhead`.
    const br = p.break === true ? 'page-before' : p.break === false ? undefined : p.break;
    let b = toBreak(br as BreakProp | undefined);
    if (p.wrap === false) b = { ...b, inside: 'avoid' };
    if (typeof p.minPresenceAhead === 'number') b = { ...b, minPresenceAhead: p.minPresenceAhead };
    const name = HOST_NAME[inst.type];
    if (isFixed(inst)) {
      this.checkFixed(inst, node.style);
      if (paginates(b)) {
        this.fail(
          inst,
          `<${name} fixed>: break, wrap={false} and minPresenceAhead do nothing on a fixed element, which is painted whole on every page. Remove them.`,
          { kind: 'props' },
        );
      }
      b = { ...b, fixed: true };
    } else if (paginates(b) && (this.band ?? this.fixedEl)) {
      const within = this.band ? `a <${this.band}>` : `a fixed <${this.fixedEl}>`;
      this.fail(
        inst,
        `<${name}>: break, wrap={false} and minPresenceAhead do nothing inside ${within}, which is painted whole on every page. Remove them.`,
        { kind: 'props' },
      );
    }
    if (b) node.break = b;
    const href = inst.type === HOST.Link ? this.target(inst) : p.href;
    if (typeof href === 'string') node.link = href;
    const bm = toBookmark(p.bookmark as BookmarkProp | undefined);
    if (bm) node.bookmark = bm;
  }

  /**
   * `fixed` repeats an element on every page of its `<Page>`: it must be placed directly in the
   * page and positioned absolutely, against the page content box, so it is in the same place on
   * every page and takes no space. Content that needs room on every page belongs in `<Header>` or
   * `<Footer>`.
   */
  private checkFixed(inst: Instance, style: Style | undefined): void {
    const name = HOST_NAME[inst.type];
    const fail = (why: string) => this.fail(inst, `<${name} fixed>: ${why}`, { kind: 'props' });
    if (this.band) {
      fail(`a <${this.band}> is already on every page, so nothing inside it can be fixed. Remove \`fixed\`.`);
    }
    if (this.inCell > 0) {
      fail('an element in a <Table> cannot be fixed. Put it directly in the <Page>.');
    }
    if (!this.onPage.has(inst)) {
      fail(
        'only an element placed directly in a <Page> can be fixed, not one inside another element. Move it out, next to the page content.',
      );
    }
    if (inst.type === HOST.Field) fail('a form field cannot be fixed.');
    if (style?.position !== 'absolute') {
      fail(
        "a fixed element needs position: 'absolute' (or the class absolute); it is placed against the page content box on every page and takes no space. For content that needs room on every page, use <Header> or <Footer>.",
      );
    }
  }

  /** A `<Link>`'s target: `href`, or its alias `src`. */
  private target(inst: Instance): string {
    const href = inst.props.href ?? inst.props.src;
    if (typeof href !== 'string' || href === '') {
      this.fail(inst, '<Link> needs an href: https://…, mailto:… or #id.', { kind: 'props' });
    }
    return href;
  }

  /** A block element's node, remembering the outermost fixed element while its content is built. */
  private blockNode(inst: Instance, ctx: TextCtx): SchemaNode {
    if (this.fixedEl === undefined && isFixed(inst)) {
      this.fixedEl = HOST_NAME[inst.type];
      try {
        return this.nodeForType(inst, ctx);
      } finally {
        this.fixedEl = undefined;
      }
    }
    return this.nodeForType(inst, ctx);
  }

  /** The node of a block element, built by the method for its type. */
  private nodeForType(inst: Instance, ctx: TextCtx): SchemaNode {
    if (typeof inst.props.render === 'function' && (inst.type === HOST.View || inst.type === HOST.Text)) {
      return this.perPage(inst, ctx);
    }
    switch (inst.type) {
      case HOST.View:
      case HOST.Link:
        return this.box(inst, ctx);
      case HOST.Text:
        return this.text(inst, ctx);
      case HOST.Field:
        return this.field(inst, ctx);
      case HOST.Svg:
      case HOST.Canvas:
        return this.drawing(inst, ctx);
      case HOST.Table:
        return this.table(inst, ctx);
      case HOST.Image:
        return this.image(inst, ctx);
      case HOST.PageBreak: {
        if (this.band ?? this.fixedEl) {
          const within = this.band ? `a <${this.band}>` : `a fixed <${this.fixedEl}>`;
          this.fail(inst, `<PageBreak> does nothing inside ${within}, which is painted whole on every page.`);
        }
        const n: SchemaNode = { type: 'pageBreak' };
        if (typeof inst.props.id === 'string') n.id = inst.props.id;
        return this.mark(n, inst);
      }
      case HOST.Spacer: {
        const own = this.styleOf(inst, ctx);
        const s: Style = { ...boxStyle(own, ctx.color) };
        const units = this.units(own, ctx);
        if (inst.props.h !== undefined) s.height = this.length(inst, inst.props.h as Length, units);
        if (inst.props.w !== undefined) s.width = this.length(inst, inst.props.w as Length, units);
        const n: SchemaNode = { type: 'spacer' };
        if (Object.keys(s).length > 0) n.style = s;
        this.flowProps(n, inst);
        return this.mark(n, inst);
      }
      default:
        return this.fail(inst, `<${HOST_NAME[inst.type] ?? inst.type}> cannot be used here.`);
    }
  }

  private box(
    inst: Instance,
    parent: TextCtx,
    content: Node[] = inst.children,
    defaults: TwStyle = {},
  ): SchemaNode {
    const own = { ...defaults, ...this.styleOf(inst, parent) };
    this.noTextOnly(inst, own);
    const ctx = inherit(parent, own);
    const node: SchemaNode = { type: 'box' };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    if (inst.type === HOST.View || inst.type === HOST.Link) this.flowProps(node, inst);
    else {
      // <Header> and <Footer>.
      if (isFixed(inst)) {
        const name = HOST_NAME[inst.type];
        this.fail(inst, `<${name} fixed>: a <${name}> is already on every page. Remove \`fixed\`.`, {
          kind: 'props',
        });
      }
      if (typeof inst.props.id === 'string') node.id = inst.props.id;
      if (inst.props.debug === true) node.debug = true;
    }
    const children = this.children(content, ctx, inst);
    if (children.length > 0) node.children = children;
    return this.mark(node, inst);
  }

  private text(inst: Instance, parent: TextCtx, content: readonly Inline[] = inst.children): TextNode {
    const own = this.styleOf(inst, parent);
    const p = inst.props;
    const ctx = inherit(parent, own, {
      lang: p.lang as string | undefined,
      hyphenate: p.hyphenate as boolean | undefined,
      hyphenation: p.hyphenationCallback as HyphenationCallback | null | undefined,
      direction: p.dir as TextCtx['direction'],
      orphans: p.orphans as number | undefined,
      widows: p.widows as number | undefined,
    });
    const node: TextNode = { type: 'text', runs: [] };
    this.runs(content, ctx, node.runs, inst);
    const style = textNodeStyle(boxStyle(own, ctx.color), own, ctx, this.docLang);
    if (style) node.style = style;
    this.flowProps(node, inst);
    return this.mark(node, inst);
  }

  /**
   * Flattens inline content into runs, merging neighbours with the same resolved style. `para`
   * is whether the paragraph uses the engine's patterns, and its language; a run that differs
   * says so itself. Runs carry no language, so a run in another language never turns the
   * patterns on: they would be the paragraph's.
   */
  private runs(
    nodes: readonly Inline[],
    ctx: TextCtx,
    out: TextRun[],
    owner: Instance,
    para: { patterns: boolean; lang: string | undefined } = { patterns: usesPatterns(ctx), lang: ctx.lang },
  ): void {
    for (const n of nodes) {
      if (n.kind === 'text') {
        if (n.text === '') continue;
        // The callback sees the words as displayed, after `textTransform`.
        const text = transformText(n.text, ctx.textTransform, ctx.lang);
        // The engine does not break `pre` and `nowrap` text, so the callback is not asked.
        const cb = ctx.whiteSpace === 'pre' || ctx.whiteSpace === 'nowrap' ? undefined : callbackOf(ctx);
        const patterns = usesPatterns(ctx);
        const hyphenate =
          patterns === para.patterns || (patterns && ctx.lang !== para.lang) ? undefined : patterns;
        const shown = cb ? this.hyphenator.apply(text, cb, (msg, init) => this.fail(owner, msg, init)) : text;
        this.push(out, shown, ctx, owner, undefined, hyphenate);
        continue;
      }
      switch (n.type) {
        case HOST.Text:
        case HOST.Link: {
          if (isFixed(n)) {
            this.fail(
              n,
              `<${HOST_NAME[n.type]} fixed>: an inline run inside a <Text> cannot be fixed. Put \`fixed\` on an element placed directly in the <Page>.`,
              { kind: 'props' },
            );
          }
          const own = this.styleOf(n, ctx);
          const box = boxKeys(own);
          if (box.length > 0 || textOnlyKeys(own).length > 0) {
            const what = n.type === HOST.Link ? 'A <Link> inside <Text>' : 'A nested <Text>';
            this.fail(
              n,
              `${what} is an inline run: only text utilities apply, not "${[...box, ...textOnlyKeys(own)].join(', ')}" (className="${n.props.className ?? ''}").`,
              {
                kind: 'tailwind',
                className: n.props.className as string | undefined,
              },
            );
          }
          const href = n.type === HOST.Link ? this.target(n) : (n.props.href as string | undefined);
          const inner =
            hasInherited(own) ||
            href !== undefined ||
            n.props.lang !== undefined ||
            n.props.hyphenate !== undefined ||
            n.props.hyphenationCallback !== undefined
              ? inherit(ctx, own, {
                  link: href,
                  lang: n.props.lang as string | undefined,
                  hyphenate: n.props.hyphenate as boolean | undefined,
                  hyphenation: n.props.hyphenationCallback as HyphenationCallback | null | undefined,
                })
              : ctx;
          this.runs(n.children, inner, out, n, para);
          break;
        }
        case HOST.PageNumber:
        case HOST.TotalPages:
          this.push(out, undefined, ctx, n, n.type === HOST.PageNumber ? 'pageNumber' : 'totalPages');
          break;
        default:
          this.fail(n, `<${HOST_NAME[n.type]}> cannot be inside <Text>.`);
      }
    }
  }

  private push(
    out: TextRun[],
    text: string | undefined,
    ctx: TextCtx,
    owner: Instance,
    placeholder: TextRun['placeholder'],
    hyphenate?: boolean,
  ): void {
    const font = this.font(ctx, owner);
    const size = ctx.fontSize;
    const ls = ctx.letterSpacing;
    const letterSpacing = ls === undefined ? 0 : 'em' in ls ? round(ls.em * size) : ls.pt;
    const decoration = ctx.textDecoration === 'none' ? undefined : ctx.textDecoration;
    const decoColor = decoration ? ctx.decorationColor : undefined;
    const decoThickness = decoration ? ctx.decorationThickness : undefined;
    const decoStyle = decoration && ctx.decorationStyle !== 'solid' ? ctx.decorationStyle : undefined;
    const last = out[out.length - 1];
    if (
      text !== undefined &&
      last &&
      last.placeholder === undefined &&
      last.font === font &&
      last.size === size &&
      sameColor(last.color, ctx.color) &&
      (last.letterSpacing ?? 0) === letterSpacing &&
      last.decoration === decoration &&
      sameColor(last.decorationColor, decoColor) &&
      last.decorationThickness === decoThickness &&
      last.decorationStyle === decoStyle &&
      last.link === ctx.link &&
      last.hyphenate === hyphenate
    ) {
      last.text += text;
      return;
    }
    const run: TextRun = placeholder ? { placeholder, font, size } : { text: text ?? '', font, size };
    run.color = ctx.color;
    if (letterSpacing !== 0) run.letterSpacing = letterSpacing;
    if (decoration) run.decoration = decoration;
    if (decoColor) run.decorationColor = decoColor;
    if (decoThickness !== undefined) run.decorationThickness = decoThickness;
    if (decoStyle) run.decorationStyle = decoStyle;
    if (ctx.link !== undefined) run.link = ctx.link;
    if (hyphenate !== undefined) run.hyphenate = hyphenate;
    out.push(this.mark(run, owner));
  }

  private table(inst: Instance, parent: TextCtx): TableNode {
    const own = this.styleOf(inst, parent);
    this.noTextOnly(inst, own);
    const ctx = inherit(parent, own);
    const cols = inst.props.columns as Array<number | string> | undefined;
    if (!Array.isArray(cols) || cols.length === 0) {
      this.fail(inst, '<Table> needs a columns prop, e.g. columns={["2fr", "1fr", 80]}.', { kind: 'props' });
    }
    const columns = cols.map((c): ColumnSpec => ({ width: c }));
    const node: TableNode = { type: 'table', columns: columns as [ColumnSpec, ...ColumnSpec[]], rows: [] };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    const headerRows = inst.props.headerRows as number | undefined;
    if (headerRows !== undefined) node.headerRows = headerRows;
    this.flowProps(node, inst);
    const rowAllowed = new Set(['backgroundColor', 'minHeight']);
    for (const r of inst.children) {
      if (r.kind !== 'element') continue;
      const rOwn = this.styleOf(r, ctx);
      this.onlyKeys(
        r,
        rOwn,
        rowAllowed,
        'A Row takes bg-*, min-h-* and text utilities; style the cells instead.',
      );
      const rctx = inherit(ctx, rOwn);
      const row: TableRow = { cells: [] };
      if (typeof r.props.id === 'string') row.id = r.props.id;
      if (r.props.debug === true) row.debug = true;
      const minHeight =
        (r.props.minHeight as number | undefined) ??
        (typeof rOwn.minHeight === 'number' ? rOwn.minHeight : undefined);
      if (minHeight !== undefined) row.minHeight = minHeight;
      if (rOwn.backgroundColor !== undefined) {
        row.backgroundColor = rOwn.backgroundColor === 'currentColor' ? rctx.color : rOwn.backgroundColor;
      }
      for (const c of r.children) {
        if (c.kind !== 'element') continue;
        row.cells.push(this.cell(c, rctx));
      }
      node.rows.push(this.mark(row, r));
    }
    return this.mark(node, inst);
  }

  private cell(inst: Instance, parent: TextCtx): TableCell {
    const own = this.styleOf(inst, parent);
    this.noTextOnly(inst, own);
    const ctx = inherit(parent, own);
    const cell: TableCell = {};
    const p = inst.props;
    if (typeof p.id === 'string') cell.id = p.id;
    if (p.debug === true) cell.debug = true;
    if (typeof p.colSpan === 'number' && p.colSpan !== 1) cell.colSpan = p.colSpan;
    if (typeof p.rowSpan === 'number' && p.rowSpan !== 1) cell.rowSpan = p.rowSpan;
    let style: Style | undefined = boxStyle(own, ctx.color, true);
    if (typeof p.verticalAlign === 'string')
      style = { ...style, verticalAlign: p.verticalAlign as Style['verticalAlign'] };
    if (style) cell.style = style;
    this.inCell++;
    let children: SchemaNode[];
    try {
      children = this.children(inst.children, ctx, inst);
    } finally {
      this.inCell--;
    }
    if (children.length > 0) cell.children = children;
    return this.mark(cell, inst);
  }

  private image(inst: Instance, ctx: TextCtx): ImageNode {
    const own = this.styleOf(inst, ctx);
    this.noTextOnly(inst, own);
    const node: ImageNode = { type: 'image' };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    const src = this.imageTable.resolve(inst.props.src, (msg) => this.fail(inst, msg, { kind: 'props' }));
    if ('data' in src) node.data = src.data;
    else node.src = src.src;
    const fit = (inst.props.fit as ImageNode['fit']) ?? own.objectFit;
    if (fit) node.fit = fit;
    if (own.objectPosition) node.objectPosition = own.objectPosition;
    this.flowProps(node, inst);
    return this.mark(node, inst);
  }

  private field(inst: Instance, parent: TextCtx): FieldNode {
    const own = this.styleOf(inst, parent);
    this.noTextOnly(inst, own);
    const ctx = inherit(parent, own);
    const p = inst.props as unknown as FieldProps;
    if (typeof p.name !== 'string' || p.name === '') {
      this.fail(inst, '<Field> needs a name.', { kind: 'props' });
    }
    const field: FieldNode['field'] = { name: p.name, kind: p.kind };
    switch (p.kind) {
      case 'text':
        if (p.value !== undefined) field.value = p.value;
        if (p.multiline) field.multiline = true;
        if (p.password) field.password = true;
        if (p.maxLength !== undefined) field.maxLength = p.maxLength;
        break;
      case 'checkbox':
        if (p.checked) field.checked = true;
        break;
      case 'radio':
        field.value = p.value;
        if (p.checked) field.checked = true;
        break;
      case 'select':
        field.options = p.options as [string, ...string[]];
        if (p.value !== undefined) field.value = p.value;
        break;
      default:
        this.fail(
          inst,
          `<Field kind="${String((p as { kind: unknown }).kind)}">: use text, checkbox, radio or select.`,
          {
            kind: 'props',
          },
        );
    }
    if (p.readOnly) field.readOnly = true;
    if (p.required) field.required = true;
    if (p.kind === 'text' || p.kind === 'select') field.font = this.font(ctx, inst);
    if (ctx.fontSize !== 10) field.size = ctx.fontSize;
    if (!sameColor(ctx.color, [0, 0, 0, 1])) field.color = ctx.color;
    const node: FieldNode = { type: 'field', field };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    const n = node as { id?: string; break?: Break; pages?: PageFilter; debug?: boolean };
    this.flowProps(n, inst);
    return this.mark(node, inst);
  }

  /** `<Svg>` and `<Canvas>`: an SVG image of the drawing, sized by `width` and `height`. */
  private drawing(inst: Instance, ctx: TextCtx): ImageNode {
    const own = this.styleOf(inst, ctx);
    this.noTextOnly(inst, own);
    const p = inst.props;
    const name = HOST_NAME[inst.type];
    if (p.width === undefined || p.height === undefined) {
      this.fail(inst, `<${name}> needs width and height.`, { kind: 'props' });
    }
    const units = this.units(own, ctx);
    const w = this.length(inst, p.width as Length, units);
    const h = this.length(inst, p.height as Length, units);
    let markup: string;
    if (inst.type === HOST.Canvas) {
      const paint = p.paint as ((painter: Painter, w: number, h: number) => void) | undefined;
      if (typeof paint !== 'function') this.fail(inst, '<Canvas> needs a paint function.', { kind: 'props' });
      const painter = new Painter();
      try {
        paint(painter, w, h);
      } catch (e) {
        this.fail(inst, `<Canvas> paint threw: ${(e as Error).message}`, { kind: 'render', cause: e });
      }
      markup = svgDocument(w, h, painter.toSvgBody());
    } else {
      markup = svgMarkup(inst, w, h, (at, msg) => this.fail(at ?? inst, msg));
    }
    const node: ImageNode = { type: 'image', data: svgDataUrl(markup) };
    const style: Style = { width: w, height: h, ...boxStyle(own, ctx.color) };
    node.style = style;
    this.flowProps(node, inst);
    return this.mark(node, inst);
  }

  /**
   * A `render` prop: the content for each page (from the previous pass's page count), grouped by
   * identical output. One group is a plain node; several become variants stacked in one grid
   * cell, each painted only on its pages.
   */
  private perPage(inst: Instance, ctx: TextCtx): SchemaNode {
    this.dynamic = true;
    const fn = inst.props.render as RenderProp;
    const total = this.opts.pageCount ?? 1;
    const groups = new Map<string, { node: SchemaNode; pages: number[] }>();
    for (let n = 1; n <= total; n++) {
      const page = { pageNumber: n, totalPages: total };
      const content = renderToInstances(
        createElement(PageContext.Provider, { value: page }, createElement(RenderCall, { fn, page })),
        inst.type,
      ).children;
      const node =
        inst.type === HOST.Text ? this.text(inst, ctx, content as Inline[]) : this.box(inst, ctx, content);
      const key = JSON.stringify(node);
      const g = groups.get(key);
      if (g) g.pages.push(n);
      else groups.set(key, { node, pages: [n] });
    }
    const all = [...groups.values()];
    if (all.length === 1 && all[0]) return all[0].node;
    return this.mark(stackVariants(all), inst);
  }
}

/** Builds the schema document from rendered host instances. */
export function buildTree(container: Container, opts: BuildOptions): BuildResult {
  const b = new Builder(opts);
  const document = b.build(container);
  return {
    document,
    images: b.imageTable.images,
    fonts: b.fontIds(),
    warnings: b.warnings,
    origin: b.origin,
    dynamic: b.dynamic,
    remote: b.imageTable.remote,
    pending: b.imageTable.pending,
  };
}

/**
 * Header and footer are laid out at the full page width in the page margin.
 * Unless they say otherwise, they line up with the page content (left and
 * right padding = the page margin) and are centred vertically in their band
 * (the top margin for the header, the bottom one for the footer).
 */
function bandDefaults(margin: PageSpec['margin'], key: 'header' | 'footer', own: TwStyle): TwStyle {
  const out: TwStyle = {};
  if (margin?.left) out.paddingLeft = margin.left;
  if (margin?.right) out.paddingRight = margin.right;
  const band = key === 'header' ? margin?.top : margin?.bottom;
  // The vertical alignment is justify-content in a column and align-items in a row.
  const row = own.flexDirection === 'row' || own.flexDirection === 'row-reverse';
  const vertical = row ? 'alignItems' : 'justifyContent';
  const placed = ['paddingTop', 'paddingBottom', 'height', 'minHeight', vertical].some(
    (k) => own[k as keyof TwStyle] !== undefined,
  );
  if (band && !placed) {
    out.minHeight = band;
    out[vertical] = 'center';
  }
  return out;
}
