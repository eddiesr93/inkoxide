import type {
  BackgroundImage,
  Bookmark,
  BoxNode,
  Break,
  Color,
  ColumnSpec,
  Document,
  FieldFormat,
  FieldNode,
  ImageNode,
  InlineImage,
  Meta,
  NoteNode,
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
import { round, type FlatStyle, type Units } from '@inkoxide/style';
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
  type Permissions,
  type RenderProp,
  type Style as StyleInput,
  type TextFormat,
} from '../components/index.ts';
import { TemplateError, type TemplateErrorInit } from '../errors.ts';
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
import { ImageTable, type ImageTableOptions, type PendingImage } from './images.ts';
import { PRESET_NAMES, presetSize } from './page-sizes.ts';
import {
  boxKeys,
  boxStyle,
  callbackOf,
  hasInherited,
  inherit,
  resolveColor,
  runAlignOf,
  type TextCtx,
  textNodeStyle,
  styleFontSize,
  textOnlyKeys,
  toPt,
  usesPatterns,
  withStyleProp,
} from './style.ts';
import { type SvgTextFonts, svgDataUrl, svgDocument, svgMarkup } from './svg.ts';
import { Hyphenator, transformText } from './text.ts';
import { type FlowKey, stackVariants } from './variants.ts';

export interface BuildOptions {
  /** The fonts, and the `hyphenationCallback` of every `Text` without its own. */
  fonts: FontRegistry;
  /** Record which instance produced each output object (for error paths). */
  track?: boolean;
  /** Page count from a previous render, for `render` props (1 when unknown). */
  pageCount?: number;
  /** The caller's image ids and policies, and the id table shared with the tree's rebuilds. */
  images: ImageTableOptions;
}

/**
 * Render options a `<Document>` sets that are not part of the document tree, for the caller to
 * pass to the engine.
 */
export interface OutputOptions {
  /** The PDF/A level of `<Document conformance>`; the render option `pdfA` wins over it. */
  pdfA?: PdfALevel;
  /**
   * The encryption of `<Document userPassword ownerPassword permissions>`; the render option
   * `encryption` replaces it. JSON and `util.inspect` show it as `'<redacted>'`.
   */
  encryption?: EncryptionOptions;
}

/** The encryption render option (the `inkoxide` binding's `EncryptionOptions`). */
export interface EncryptionOptions {
  userPassword?: string;
  ownerPassword?: string;
  permissions?: Permissions;
}

const REDACTED = '<redacted>';

/** Output options whose JSON and `util.inspect` view hide the passwords. */
function outputOptions(): OutputOptions {
  const redacted = function (this: OutputOptions): object {
    return this.encryption === undefined ? { ...this } : { ...this, encryption: REDACTED };
  };
  const out: OutputOptions = {};
  for (const key of ['toJSON', Symbol.for('nodejs.util.inspect.custom')]) {
    Object.defineProperty(out, key, { value: redacted, enumerable: false });
  }
  return out;
}

/** A PDF/A part and conformance level the engine writes. */
export type PdfALevel = '1b' | '2b' | '3b';

export interface BuildResult {
  document: Document;
  /** Render options the `<Document>` sets (`conformance`); not part of `document`. */
  output: OutputOptions;
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

/**
 * Props of a `Text` or `Link` that need a node of its own: an inline run inside a `Text` has
 * none. `debug` is left out: on a run it draws nothing, as the guide says.
 */
const BLOCK_ONLY = [
  'id',
  'bookmark',
  'pages',
  'render',
  'dir',
  'orphans',
  'widows',
  'break',
  'wrap',
  'minPresenceAhead',
] as const;

/** Whether a block-only prop is set to something other than its default (`wrap`, `break={false}`). */
function blockOnly(props: Record<string, unknown>, key: (typeof BLOCK_ONLY)[number]): boolean {
  const v = props[key];
  return v !== undefined && !(key === 'wrap' && v === true) && !(key === 'break' && v === false);
}

const isEl = (n: Node | undefined, type: string): n is Instance => n?.kind === 'element' && n.type === type;

/** A run's OpenType features: `fontVariantNumeric`'s, then `fontFeatureSettings`, which win. */
function fontFeatures(ctx: TextCtx): Record<string, number> | undefined {
  const { fontVariantNumeric: numeric, fontFeatureSettings: settings } = ctx;
  if (!numeric && !settings) return undefined;
  const out = { ...numeric, ...settings };
  return Object.keys(out).length > 0 ? out : undefined;
}

function sameFeatures(a: TextRun['fontFeatures'], b: TextRun['fontFeatures']): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => a[k] === b[k]);
}

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

/** Copies the props every element's node takes, `id` and `debug`. */
function commonProps(node: { id?: string; debug?: boolean }, inst: Instance): void {
  if (typeof inst.props.id === 'string') node.id = inst.props.id;
  if (inst.props.debug === true) node.debug = true;
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

/** An emoji sequence (Unicode's RGI emoji); the `v` flag is built at run time for the ES2023 target. */
const EMOJI = new RegExp('\\p{RGI_Emoji}', 'gv');

class Builder {
  readonly imageTable: ImageTable;
  /** The document's `emojiImages`: emoji sequence (without U+FE0F) → image id. */
  readonly emoji: Record<string, string> = {};
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
  /** The names of the `<FieldSet>`s being built, each followed by `.`: `address.`. */
  private fieldPrefix = '';
  /** The style of each element's `className`, to tell which keys came from it. */
  private readonly classStyles = new WeakMap<Instance, FlatStyle>();

  constructor(private readonly opts: BuildOptions) {
    this.origin = opts.track ? new Map() : undefined;
    this.imageTable = new ImageTable(opts.images);
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
  private styleOf(inst: Instance, parent: TextCtx): FlatStyle {
    const className = inst.props.className as string | undefined;
    const style = inst.props.style as StyleInput | undefined;
    const invalidStyle = (e: unknown): never =>
      this.fail(inst, `Invalid style prop on <${HOST_NAME[inst.type]}>: ${(e as Error).message}`, {
        kind: 'props',
        cause: e,
      });
    let s: FlatStyle = {};
    if (className?.trim()) {
      // `em` in the classes is the element's font size, which its `style` may set.
      let fontSize: number | undefined;
      try {
        fontSize = style && styleFontSize(style, parent.fontSize, this.pageBox);
      } catch (e) {
        invalidStyle(e);
      }
      let classes: FlatStyle | string;
      try {
        classes = classStyle(className, { parentFontSize: parent.fontSize, fontSize, page: this.pageBox });
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
      this.classStyles.set(inst, classes);
    }
    if (style) {
      try {
        s = withStyleProp(s, style, parent.fontSize, this.pageBox);
      } catch (e) {
        invalidStyle(e);
      }
    }
    if ((s.position as string | undefined) === 'fixed') {
      this.fail(
        inst,
        `<${HOST_NAME[inst.type]}>: position: 'fixed' is not supported; to repeat an element on every page, give it the fixed prop and position: 'absolute'.`,
        { kind: 'props' },
      );
    }
    if (inst.type !== HOST.Image && inst.type !== HOST.ImageBackground) {
      for (const key of ['objectFit', 'objectPosition'] as const) {
        if (s[key] !== undefined) {
          this.fail(inst, `${key} applies to <Image> and <ImageBackground>, not <${HOST_NAME[inst.type]}>.`, {
            kind: 'props',
          });
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

  /**
   * Where the value of `key` in an element's own style `s` came from: its `className` (with the
   * error fields that go with a class), or its `style` prop.
   */
  private source(inst: Instance, s: FlatStyle, key: string): { fromClass: boolean; init: TemplateErrorInit } {
    const v = this.classStyles.get(inst)?.[key as keyof FlatStyle];
    if (v === undefined || v !== s[key as keyof FlatStyle])
      return { fromClass: false, init: { kind: 'props' } };
    return { fromClass: true, init: { kind: 'tailwind', className: inst.props.className as string } };
  }

  private onlyKeys(inst: Instance, s: FlatStyle, allowed: ReadonlySet<string>, hint: string): void {
    for (const k of boxKeys(s)) {
      if (!allowed.has(k)) {
        const { fromClass, init } = this.source(inst, s, k);
        const from = fromClass ? `className="${init.className}"` : 'its style';
        this.fail(inst, `<${HOST_NAME[inst.type]}> does not support "${k}" (from ${from}). ${hint}`, init);
      }
    }
  }

  private noTextOnly(inst: Instance, s: FlatStyle): void {
    const [k, ...more] = textOnlyKeys(s);
    if (k === undefined) return;
    const { fromClass, init } = this.source(inst, s, k);
    const name = HOST_NAME[inst.type];
    this.fail(
      inst,
      fromClass
        ? `truncate and line-clamp-* apply to <Text>, not <${name}> (className="${init.className}").`
        : `${[k, ...more].join(' and ')} ${more.length > 0 ? 'apply' : 'applies'} to <Text>, not <${name}> (from its style).`,
      init,
    );
  }

  /** What the `<Document>` sets besides the tree; filled by `build`. */
  readonly output: OutputOptions = outputOptions();

  build(container: Container): Document {
    const [root, ...extra] = container.children.filter((c) => !(c.kind === 'text' && c.text.trim() === ''));
    if (!isEl(root, HOST.Document) || extra.length > 0) {
      throw new TemplateError('A template must render exactly one <Document> at the top.');
    }
    const p = root.props;
    const meta: Meta = {};
    for (const k of [
      'title',
      'author',
      'subject',
      'keywords',
      'creator',
      'producer',
      'lang',
      'creationDate',
    ] as const) {
      const v = p[k];
      if (typeof v === 'string') meta[k] = v;
    }
    if (p.conformance !== undefined) this.output.pdfA = this.conformance(root, p.conformance);
    const encryption = this.encryption(root, p);
    if (encryption) this.output.encryption = encryption;
    // Checked against the schema's values with the rest of the document.
    if (typeof p.pageMode === 'string') meta.pageMode = p.pageMode as Meta['pageMode'];
    if (typeof p.pageLayout === 'string') meta.pageLayout = p.pageLayout as Meta['pageLayout'];
    this.docLang = meta.lang;
    this.debugAll = p.debug === true;
    const ctx: TextCtx = {
      fontFamily: defaultFamily(),
      fontWeight: 400,
      fontStyle: 'normal',
      fontSize: 12,
      color: [0, 0, 0, 1],
      lang: meta.lang,
      hyphenation: this.opts.fonts.hyphenationCallback,
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
      ...(Object.keys(this.emoji).length > 0 ? { emojiImages: this.emoji } : {}),
      pages: pages as [PageSpec, ...PageSpec[]],
    };
    return this.mark(doc, root);
  }

  /**
   * The encryption of `<Document userPassword ownerPassword permissions>`, its types checked;
   * the engine checks the values. Messages never show a password.
   */
  private encryption(root: Instance, p: Record<string, unknown>): EncryptionOptions | undefined {
    const out: EncryptionOptions = {};
    for (const key of ['userPassword', 'ownerPassword'] as const) {
      const v = p[key];
      if (v === undefined) continue;
      if (typeof v !== 'string') this.fail(root, `<Document ${key}> must be a string.`);
      out[key] = v;
    }
    if (p.permissions !== undefined) {
      const perms = p.permissions;
      if (typeof perms !== 'object' || perms === null || Array.isArray(perms)) {
        this.fail(root, '<Document permissions> must be an object.');
      }
      out.permissions = { ...(perms as Permissions) };
    }
    return Object.keys(out).length > 0 ? out : undefined;
  }

  /** The PDF/A level of `<Document conformance>`; a bare part (`'PDF/A-2'`) is level b. */
  private conformance(root: Instance, value: unknown): PdfALevel {
    const m = typeof value === 'string' ? /^PDF\/A-([123])([abu]?)$/.exec(value) : null;
    if (m && (m[2] === '' || m[2] === 'b')) return `${m[1]}b` as PdfALevel;
    const shown = typeof value === 'string' ? `"${value}"` : `{${String(value)}}`;
    if (m) {
      this.fail(
        root,
        `<Document conformance=${shown}>: the a and u levels need tagged PDF, which is not supported yet; use 'PDF/A-${m[1]}b'.`,
      );
    }
    this.fail(
      root,
      `<Document conformance=${shown}> is not a PDF/A level: use 'PDF/A-1b', 'PDF/A-2b' or 'PDF/A-3b' (or 'PDF/A-1', 'PDF/A-2', 'PDF/A-3').`,
    );
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
    this.noTextOnly(inst, own);
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
      const v = this.length(inst, 'margin', m, units);
      spec.margin = { top: v, right: v, bottom: v, left: v };
    } else if (m) {
      spec.margin = {};
      for (const k of ['top', 'right', 'bottom', 'left'] as const) {
        const v = m[k];
        if (v !== undefined) spec.margin[k] = this.length(inst, 'margin', v, units);
      }
    }
    // Padding fills the sides the margin prop leaves unset. A percentage is of the page width on
    // every side, as padding is of the containing block's width in CSS.
    const pad = {
      top: own.paddingTop,
      right: own.paddingRight,
      bottom: own.paddingBottom,
      left: own.paddingLeft,
    };
    for (const k of ['top', 'right', 'bottom', 'left'] as const) {
      const v = pad[k];
      if (v === undefined || spec.margin?.[k] !== undefined) continue;
      const pt = typeof v === 'number' ? v : round((Number.parseFloat(v) * size.width) / 100);
      spec.margin = { ...spec.margin, [k]: pt };
    }
    if (own.backgroundColor !== undefined) {
      spec.background = resolveColor(own.backgroundColor, ctx.color);
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
      const width = this.length(inst, 'size', w);
      return { width, height: h === undefined ? width : this.length(inst, 'size', h) };
    }
    if (typeof size === 'number') {
      const side = this.length(inst, 'size', size);
      return { width: side, height: side };
    }
    if (typeof size === 'string') {
      const preset = presetSize(size);
      if (preset) return preset;
      let side: unknown;
      try {
        side = toPt('size', size);
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
    return { width: this.length(inst, 'size', size.width), height: this.length(inst, 'size', size.height) };
  }

  /** Block children; runs of bare strings become one anonymous text node. */
  private children(nodes: Node[], ctx: TextCtx, parent: Instance): SchemaNode[] {
    const out: SchemaNode[] = [];
    let pending: Node[] | undefined;
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
  private units(own: FlatStyle, parent: TextCtx): Units {
    return { em: own.fontSize ?? parent.fontSize, page: this.pageBox };
  }

  /** Points from the length prop `prop` (`12`, `'20mm'`); `em`, `vw` and `vh` need `units`. */
  private length(inst: Instance, prop: string, v: Length, units?: Units): number {
    try {
      return toPt(prop, v, units);
    } catch (e) {
      return this.fail(inst, `<${HOST_NAME[inst.type]}>: ${(e as Error).message}.`, { kind: 'props' });
    }
  }

  /**
   * The props of an element in the flow. A per-page wrapper (`stackVariants`) takes the ones in
   * `FlowKey` from its variants, so a prop written here belongs there or with `style` and `debug`.
   */
  private flowProps(
    node: Partial<Pick<BoxNode, FlowKey>> & { style?: Style; debug?: boolean },
    inst: Instance,
  ): void {
    const p = inst.props;
    commonProps(node, inst);
    if (p.pages !== undefined) node.pages = p.pages as PageFilter;
    // Aliases for the same behaviour: `wrap`, `break={true}` and `minPresenceAhead`.
    const br = p.break === true ? 'page-before' : p.break === false ? undefined : p.break;
    let b = toBreak(br as BreakProp | undefined);
    if (p.wrap === false) b = { ...b, inside: 'avoid' };
    if (typeof p.minPresenceAhead === 'number') b = { ...b, minPresenceAhead: p.minPresenceAhead };
    const name = HOST_NAME[inst.type];
    const within = this.repeatedContainer();
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
    } else if (paginates(b) && within) {
      this.fail(
        inst,
        `<${name}>: break, wrap={false} and minPresenceAhead do nothing inside ${within}. Remove them.`,
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
   * The band or fixed element being built, for messages: "a <Header>, which is painted whole on
   * every page".
   */
  private repeatedContainer(): string | undefined {
    const within = this.band ? `a <${this.band}>` : this.fixedEl ? `a fixed <${this.fixedEl}>` : undefined;
    return within && `${within}, which is painted whole on every page`;
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
      case HOST.ImageBackground:
      case HOST.Link:
        return this.box(inst, ctx);
      case HOST.FieldSet:
        return this.fieldSet(inst, ctx);
      case HOST.Note:
        return this.note(inst, ctx);
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
        const within = this.repeatedContainer();
        if (within) this.fail(inst, `<PageBreak> does nothing inside ${within}.`);
        const n: SchemaNode = { type: 'pageBreak' };
        if (typeof inst.props.id === 'string') n.id = inst.props.id;
        return this.mark(n, inst);
      }
      case HOST.Spacer: {
        const own = this.styleOf(inst, ctx);
        this.noTextOnly(inst, own);
        const s: Style = { ...boxStyle(own, ctx.color) };
        const units = this.units(own, ctx);
        if (inst.props.h !== undefined) s.height = this.length(inst, 'h', inst.props.h as Length, units);
        if (inst.props.w !== undefined) s.width = this.length(inst, 'w', inst.props.w as Length, units);
        const n: SchemaNode = { type: 'spacer' };
        if (Object.keys(s).length > 0) n.style = s;
        this.flowProps(n, inst);
        return this.mark(n, inst);
      }
      default:
        throw new Error(`Internal error: the reconciler let <${inst.type}> into the flow.`);
    }
  }

  private box(
    inst: Instance,
    parent: TextCtx,
    content: Node[] = inst.children,
    defaults: FlatStyle = {},
  ): SchemaNode {
    const own = { ...defaults, ...this.styleOf(inst, parent) };
    this.noTextOnly(inst, own);
    const ctx = inherit(parent, own);
    const node: BoxNode = { type: 'box' };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    if (inst.type === HOST.ImageBackground) node.backgroundImage = this.backgroundImage(inst, own);
    if (
      inst.type === HOST.View ||
      inst.type === HOST.ImageBackground ||
      inst.type === HOST.Link ||
      inst.type === HOST.FieldSet
    ) {
      this.flowProps(node, inst);
    } else {
      // <Header> and <Footer>.
      if (isFixed(inst)) {
        const name = HOST_NAME[inst.type];
        this.fail(inst, `<${name} fixed>: a <${name}> is already on every page. Remove \`fixed\`.`, {
          kind: 'props',
        });
      }
      commonProps(node, inst);
    }
    const children = this.children(content, ctx, inst);
    if (children.length > 0) node.children = children;
    return this.mark(node, inst);
  }

  private text(inst: Instance, parent: TextCtx, content: readonly Node[] = inst.children): TextNode {
    const own = this.styleOf(inst, parent);
    const p = inst.props;
    const ctx = inherit(parent, own, {
      verticalAlign: runAlignOf(own),
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
   * says so itself. Runs carry no language, so a run in another language never uses the
   * patterns: they would be the paragraph's.
   */
  private runs(
    nodes: readonly Node[],
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
        const patterns = usesPatterns(ctx) && ctx.lang === para.lang;
        const hyphenate = patterns === para.patterns ? undefined : patterns;
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
          const what = n.type === HOST.Link ? 'A <Link> inside <Text>' : 'A nested <Text>';
          const prop = BLOCK_ONLY.find((k) => blockOnly(n.props, k));
          if (prop) {
            this.fail(
              n,
              `${what} is an inline run of the paragraph, so \`${prop}\` has nothing to apply to. Put it on the outer <Text>, or make this text a <Text> of its own.`,
              { kind: 'props' },
            );
          }
          const own = this.styleOf(n, ctx);
          // `verticalAlign` places the run: only a cell's values are box keys.
          const align = runAlignOf(own);
          const bad = [...boxKeys(own), ...textOnlyKeys(own)].filter(
            (k) => k !== 'verticalAlign' || align === undefined,
          );
          if (bad[0] !== undefined) {
            const { fromClass, init } = this.source(n, own, bad[0]);
            this.fail(
              n,
              fromClass
                ? `${what} is an inline run: only text utilities apply, not "${bad.join(', ')}" (className="${init.className}").`
                : `${what} is an inline run: only text styles apply, not "${bad.join(', ')}" (from its style).`,
              init,
            );
          }
          const href = n.type === HOST.Link ? this.target(n) : (n.props.href as string | undefined);
          const inner =
            hasInherited(own) ||
            align !== undefined ||
            href !== undefined ||
            n.props.lang !== undefined ||
            n.props.hyphenate !== undefined ||
            n.props.hyphenationCallback !== undefined
              ? inherit(ctx, own, {
                  link: href,
                  verticalAlign: align,
                  lang: n.props.lang as string | undefined,
                  hyphenate: n.props.hyphenate as boolean | undefined,
                  hyphenation: n.props.hyphenationCallback as HyphenationCallback | null | undefined,
                })
              : ctx;
          this.runs(n.children, inner, out, n, para);
          break;
        }
        case HOST.Image:
          out.push(this.inlineImage(n, ctx));
          break;
        case HOST.PageNumber:
        case HOST.TotalPages:
          this.push(out, undefined, ctx, n, n.type === HOST.PageNumber ? 'pageNumber' : 'totalPages');
          break;
        default:
          throw new Error(`Internal error: the reconciler let <${n.type}> into a <Text>.`);
      }
    }
  }

  /**
   * An `<Image>` in a `<Text>`: a run of its own with the image in place of text, sized by its
   * `width` and `height` (points or em; with neither, 1em high), placed by `verticalAlign`.
   */
  private inlineImage(n: Instance, ctx: TextCtx): TextRun {
    const init = { kind: 'props' } as const;
    const prop =
      BLOCK_ONLY.find((k) => blockOnly(n.props, k)) ??
      (['fixed', 'href'] as const).find((k) => n.props[k] !== undefined);
    if (prop) {
      this.fail(
        n,
        `An <Image> inside <Text> is part of the line, so \`${prop}\` does not apply to it.${prop === 'href' ? ' Wrap it in a <Link>.' : ''}`,
        init,
      );
    }
    const own = this.styleOf(n, ctx);
    const allowed = new Set(['width', 'height', 'verticalAlign']);
    const bad = [...boxKeys(own), ...textOnlyKeys(own)].filter((k) => !allowed.has(k));
    if (bad[0] !== undefined) {
      const { fromClass, init: from } = this.source(n, own, bad[0]);
      this.fail(
        n,
        `An <Image> inside <Text> is part of the line: it takes width, height, objectFit, objectPosition and verticalAlign, not "${bad.join(', ')}" (from ${fromClass ? `className="${from.className}"` : 'its style'}).`,
        from,
      );
    }
    const src = this.imageTable.resolve(n.props.src, (msg) => this.fail(n, msg, init));
    const image: InlineImage = 'data' in src ? { data: src.data } : { src: src.src };
    for (const side of ['width', 'height'] as const) {
      const v = own[side];
      if (v === undefined || v === 'auto') continue;
      if (typeof v !== 'number') {
        this.fail(
          n,
          `An <Image> inside <Text> takes its ${side} in points or em, not ${String(v)}: a line has no size to take a percentage of.`,
          init,
        );
      }
      image[side] = v;
    }
    const fit = (n.props.fit as InlineImage['fit']) ?? own.objectFit;
    if (fit) image.fit = fit;
    if (own.objectPosition) image.objectPosition = own.objectPosition;
    if (typeof n.props.alt === 'string' && n.props.alt !== '') image.alt = n.props.alt;
    const run: TextRun = { image, font: this.font(ctx, n), size: ctx.fontSize };
    const align = own.verticalAlign ?? ctx.verticalAlign;
    if (align !== undefined && align !== 'baseline') run.verticalAlign = align as TextRun['verticalAlign'];
    if (ctx.link !== undefined) run.link = ctx.link;
    return this.mark(run, n);
  }

  /**
   * With an emoji source registered, the image of each emoji sequence in `text` (by its sequence
   * without U+FE0F) goes to the document's `emojiImages`, resolved like an `<Image src>`.
   */
  private emojiOf(text: string, owner: Instance): void {
    const source = this.opts.fonts.emojiSource;
    // Every emoji is outside ASCII (a keycap ends in U+20E3).
    if (!source || !/[\u0080-\u{10FFFF}]/u.test(text)) return;
    for (const [sequence] of text.matchAll(EMOJI)) {
      const key = sequence.replaceAll('\uFE0F', '');
      if (Object.hasOwn(this.emoji, key)) continue;
      const points = (s: string) => [...s].map((c) => c.codePointAt(0)?.toString(16)).join('-');
      let src: unknown;
      if ('builder' in source) {
        src = source.builder(points(key));
      } else {
        const named = source.withVariationSelectors ? sequence : key;
        src = `${source.url}${points(named)}.${source.format ?? 'png'}`;
      }
      const fail = (msg: string): never =>
        this.fail(owner, `Font.registerEmojiSource, for ${sequence}: ${msg}`, { kind: 'props' });
      let image = this.imageTable.resolve(src, fail);
      if ('data' in image) {
        // `emojiImages` takes image ids: a data: URL becomes bytes with an id of their own.
        const body = /;base64,(.*)$/s.exec(image.data)?.[1]?.replace(/\s/g, '');
        if (body === undefined || !/^[A-Za-z0-9+/]*={0,2}$/.test(body)) {
          return fail('a data: URL that is not valid base64 (`data:<type>;base64,…`)');
        }
        image = this.imageTable.resolve(
          Uint8Array.from(atob(body), (c) => c.charCodeAt(0)),
          fail,
        );
      }
      if ('src' in image) this.emoji[key] = image.src;
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
    if (text !== undefined) this.emojiOf(text, owner);
    const font = this.font(ctx, owner);
    const size = ctx.fontSize;
    const ls = ctx.letterSpacing;
    const letterSpacing = ls === undefined ? 0 : 'em' in ls ? round(ls.em * size) : ls.pt;
    const decoration = ctx.textDecoration === 'none' ? undefined : ctx.textDecoration;
    const decoColor = decoration ? ctx.decorationColor : undefined;
    const decoThickness = decoration ? ctx.decorationThickness : undefined;
    const decoStyle = decoration && ctx.decorationStyle !== 'solid' ? ctx.decorationStyle : undefined;
    const verticalAlign = ctx.verticalAlign === 'baseline' ? undefined : ctx.verticalAlign;
    const features = fontFeatures(ctx);
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
      last.hyphenate === hyphenate &&
      last.verticalAlign === verticalAlign &&
      sameFeatures(last.fontFeatures, features)
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
    if (verticalAlign !== undefined) run.verticalAlign = verticalAlign;
    if (features !== undefined) run.fontFeatures = features;
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
      this.noTextOnly(r, rOwn);
      this.onlyKeys(
        r,
        rOwn,
        rowAllowed,
        'A Row takes a background, a minimum height and text styles; style the cells instead.',
      );
      const rctx = inherit(ctx, rOwn);
      const row: TableRow = { cells: [] };
      commonProps(row, r);
      let minHeight = r.props.minHeight as number | undefined;
      if (minHeight === undefined && typeof rOwn.minHeight === 'number') minHeight = rOwn.minHeight;
      else if (minHeight === undefined && typeof rOwn.minHeight === 'string' && rOwn.minHeight !== 'auto') {
        const { fromClass, init } = this.source(r, rOwn, 'minHeight');
        this.fail(
          r,
          `<Row>: a minimum height of ${rOwn.minHeight} (from ${fromClass ? `className="${init.className}"` : 'its style'}) has no table height to be a percentage of; use a length.`,
          init,
        );
      }
      if (minHeight !== undefined) row.minHeight = minHeight;
      if (rOwn.backgroundColor !== undefined) {
        row.backgroundColor = resolveColor(rOwn.backgroundColor, rctx.color);
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
    commonProps(cell, inst);
    if (typeof p.colSpan === 'number' && p.colSpan !== 1) cell.colSpan = p.colSpan;
    if (typeof p.rowSpan === 'number' && p.rowSpan !== 1) cell.rowSpan = p.rowSpan;
    if (runAlignOf(own) !== undefined) {
      const { fromClass, init } = this.source(inst, own, 'verticalAlign');
      this.fail(
        inst,
        `<Cell>: verticalAlign "${own.verticalAlign}" (from ${fromClass ? `className="${init.className}"` : 'its style'}) places text on its line; a Cell takes top, middle or bottom.`,
        init,
      );
    }
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

  /** An `<ImageBackground>`'s image: its `src`, with `fit` and `objectPosition` from its props and style. */
  private backgroundImage(inst: Instance, own: FlatStyle): BackgroundImage {
    const fail = (msg: string): never => this.fail(inst, msg, { kind: 'props' });
    for (const key of ['source', 'imageStyle', 'srcSet']) {
      if (inst.props[key] !== undefined) {
        fail(
          `<ImageBackground> does not take ${key}: pass the image as src, and place it with fit and objectPosition (style or object-* classes).`,
        );
      }
    }
    const src = this.imageTable.resolve(inst.props.src, fail, 'ImageBackground');
    const bg: BackgroundImage = 'data' in src ? { data: src.data } : { src: src.src };
    const fit = (inst.props.fit as BackgroundImage['fit']) ?? own.objectFit;
    if (fit) bg.fit = fit;
    if (own.objectPosition) bg.objectPosition = own.objectPosition;
    return bg;
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
    const field: FieldNode['field'] = { name: this.fieldPrefix + p.name, kind: p.kind };
    switch (p.kind) {
      case 'text':
        if (p.value !== undefined) field.value = p.value;
        if (p.multiline) field.multiline = true;
        if (p.password) field.password = true;
        if (p.maxLength !== undefined) field.maxLength = p.maxLength;
        if (p.format !== undefined) field.format = this.textFormat(inst, p.format);
        break;
      case 'checkbox':
        if (p.checked) field.checked = true;
        break;
      case 'radio':
        field.value = p.value;
        if (p.checked) field.checked = true;
        break;
      case 'select':
      case 'list':
        field.options = (p.options ?? p.select) as [string, ...string[]];
        if (p.kind === 'list') {
          if (p.multiSelect) field.multiSelect = true;
          if (Array.isArray(p.value) && !p.multiSelect) {
            this.fail(inst, '<List value={[…]}>: several selected options need multiSelect.', {
              kind: 'props',
            });
          }
        }
        if (p.value !== undefined) field.value = p.value;
        break;
      default:
        this.fail(
          inst,
          `<Field kind="${String((p as { kind: unknown }).kind)}">: use text, checkbox, radio, select or list.`,
          {
            kind: 'props',
          },
        );
    }
    if (p.readOnly) field.readOnly = true;
    if (p.required) field.required = true;
    if (p.kind === 'text' || p.kind === 'select' || p.kind === 'list') field.font = this.font(ctx, inst);
    if (ctx.fontSize !== 10) field.size = ctx.fontSize;
    if (!sameColor(ctx.color, [0, 0, 0, 1])) field.color = ctx.color;
    const node: FieldNode = { type: 'field', field };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    this.flowProps(node, inst);
    return this.mark(node, inst);
  }

  /**
   * What the text of a drawing in `ctx` needs: the text style around it, and the registered face
   * for a font, which the document then declares.
   */
  private svgText(ctx: TextCtx): SvgTextFonts {
    return {
      font: { family: ctx.fontFamily, weight: ctx.fontWeight, style: ctx.fontStyle },
      size: ctx.fontSize,
      face: (font, inst) => {
        const own = { ...ctx, fontFamily: font.family, fontWeight: font.weight, fontStyle: font.style };
        const decl = this.opts.fonts.decl(this.font({ ...own, fontId: undefined }, inst));
        return { family: [decl.family], weight: decl.weight ?? 400, style: decl.style ?? 'normal' };
      },
      fail: (inst, message) => this.fail(inst, message, { kind: 'props' }),
    };
  }

  /** A `<Note>`: its text children are its contents, and its background colour its colour. */
  private note(inst: Instance, ctx: TextCtx): NoteNode {
    const own = this.styleOf(inst, ctx);
    this.noTextOnly(inst, own);
    const contents = inst.children.map((c) => (c.kind === 'text' ? c.text : '')).join('');
    if (contents === '') {
      this.fail(inst, '<Note> needs text: <Note>Check the totals</Note>.', { kind: 'props' });
    }
    const p = inst.props;
    const note: NoteNode['note'] = { contents };
    if (typeof p.author === 'string') note.author = p.author;
    if (p.open) note.open = true;
    const node: NoteNode = { type: 'note', note };
    const { backgroundColor, ...style } = boxStyle(own, ctx.color) ?? {};
    if (backgroundColor) note.color = backgroundColor;
    if (Object.keys(style).length > 0) node.style = style;
    this.flowProps(node, inst);
    return this.mark(node, inst);
  }

  /** A `TextField`'s format, from its readable keys or their other names. */
  private textFormat(inst: Instance, f: TextFormat): FieldFormat {
    if (typeof f !== 'object' || f === null || Array.isArray(f)) {
      this.fail(inst, "<TextField format> is an object, as format={{ type: 'number', decimals: 2 }}.", {
        kind: 'props',
      });
    }
    const type = f.type === 'zip4' ? 'zipPlus4' : f.type;
    const out: FieldFormat = { type };
    // The other names come with their own defaults: no decimals and no thousands separator.
    const aliased = ALIAS_KEYS.some((k) => f[k] !== undefined) && (type === 'number' || type === 'percent');
    const decimals = f.decimals ?? f.nDec ?? (aliased ? 0 : undefined);
    if (decimals !== undefined) out.decimals = decimals;
    const sepComma = f.sepComma ?? (aliased ? false : undefined);
    const separators =
      f.separators ?? (sepComma === undefined ? undefined : sepComma ? '1,234.56' : '1234.56');
    if (separators !== undefined) out.separators = separators;
    const negative = f.negative ?? (f.negStyle === undefined ? undefined : NEGATIVE[f.negStyle]);
    if (negative !== undefined) out.negative = negative;
    if (f.currency !== undefined) out.currency = f.currency;
    const before = f.currencyBefore ?? f.currencyPrepend;
    if (before !== undefined) out.currencyBefore = before;
    const pattern = f.pattern ?? (f.param === undefined ? undefined : this.formatParam(inst, type, f.param));
    if (pattern !== undefined) out.pattern = pattern;
    return out;
  }

  /** A format's `param`: the pattern of a date, or one of the four styles of a time, by index. */
  private formatParam(inst: Instance, type: FieldFormat['type'], param: string | number): string {
    if (type !== 'time') return String(param);
    const style = /^[0-3]$/.test(String(param)) ? TIME_STYLES[Number(param)] : undefined;
    if (style === undefined) {
      this.fail(
        inst,
        `<TextField format> param on a time is a style from 0 to 3, not ${JSON.stringify(param)}; ` +
          "give a pattern as pattern: 'HH:MM'.",
        { kind: 'props' },
      );
    }
    return style;
  }

  /** A `<FieldSet>`: a box whose name goes in front of the names of the fields inside it. */
  private fieldSet(inst: Instance, ctx: TextCtx): SchemaNode {
    const name = inst.props.name;
    if (typeof name !== 'string' || name === '') {
      this.fail(inst, '<FieldSet> needs a name: the group its fields are in.', { kind: 'props' });
    }
    const outer = this.fieldPrefix;
    this.fieldPrefix = `${outer}${name}.`;
    try {
      return this.box(inst, ctx);
    } finally {
      this.fieldPrefix = outer;
    }
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
    const w = this.length(inst, 'width', p.width as Length, units);
    const h = this.length(inst, 'height', p.height as Length, units);
    let markup: string;
    if (inst.type === HOST.Canvas) {
      const paint = p.paint as ((painter: Painter, w: number, h: number) => void) | undefined;
      if (typeof paint !== 'function') this.fail(inst, '<Canvas> needs a paint function.', { kind: 'props' });
      const text = this.svgText(ctx);
      const painter = new Painter({
        font: text.font,
        byId: (id) => {
          if (!this.opts.fonts.has(id)) return undefined;
          const d = this.opts.fonts.decl(id);
          return { family: [d.family], weight: d.weight ?? 400, style: d.style ?? 'normal' };
        },
        face: (font) => text.face(font, inst),
      });
      try {
        paint(painter, w, h);
      } catch (e) {
        this.fail(inst, `<Canvas> paint threw: ${(e as Error).message}`, { kind: 'render', cause: e });
      }
      markup = svgDocument(w, h, painter.toSvgBody());
    } else {
      markup = svgMarkup(inst, w, h, this.svgText(ctx));
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
      const node = inst.type === HOST.Text ? this.text(inst, ctx, content) : this.box(inst, ctx, content);
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

/** The other names of format keys, which come with their own defaults. */
const ALIAS_KEYS = ['nDec', 'sepComma', 'negStyle', 'currencyPrepend', 'param'] as const;

/** The patterns of a time format's `param`: the four styles of `AFTime_Format`. */
const TIME_STYLES: readonly string[] = ['HH:MM', 'h:MM tt', 'HH:MM:ss', 'h:MM:ss tt'];

/** The negative styles by their other names (`negStyle`). */
const NEGATIVE: Record<string, FieldFormat['negative']> = {
  MinusBlack: 'minus',
  Red: 'red',
  ParensBlack: 'parens',
  ParensRed: 'parensRed',
};

/** Builds the schema document from rendered host instances. */
export function buildTree(container: Container, opts: BuildOptions): BuildResult {
  const b = new Builder(opts);
  const document = b.build(container);
  return {
    document,
    output: b.output,
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
function bandDefaults(margin: PageSpec['margin'], key: 'header' | 'footer', own: FlatStyle): FlatStyle {
  const out: FlatStyle = {};
  if (margin?.left) out.paddingLeft = margin.left;
  if (margin?.right) out.paddingRight = margin.right;
  const band = key === 'header' ? margin?.top : margin?.bottom;
  // The vertical alignment is justify-content in a column and align-items in a row.
  const row = own.flexDirection === 'row' || own.flexDirection === 'row-reverse';
  const vertical = row ? 'alignItems' : 'justifyContent';
  const placed = ['paddingTop', 'paddingBottom', 'height', 'minHeight', vertical].some(
    (k) => own[k as keyof FlatStyle] !== undefined,
  );
  if (band && !placed) {
    out.minHeight = band;
    out[vertical] = 'center';
  }
  return out;
}
