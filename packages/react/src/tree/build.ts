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
import { type TwStyle, tw, UnsupportedUtilityError } from '@inkoxide/tailwind';
import { createElement } from 'react';

import { Painter } from '../canvas.ts';
import {
  type BookmarkProp,
  type BreakProp,
  type FieldProps,
  HOST,
  HOST_NAME,
  type HyphenationCallback,
  type ImageRequest,
  type ImageSrc,
  type ImageSrcValue,
  type Length,
  type PageSize,
  type RenderProp,
  type Style as StyleInput,
} from '../components/index.ts';
import { TemplateError } from '../errors.ts';
import type { FontRegistry } from '../fonts/registry.ts';
import { PageContext, RenderCall } from '../page.ts';
import { type Container, componentStack, type Instance, type Node } from '../reconciler/instance.ts';
import { renderToInstances } from '../reconciler/render.ts';
import {
  boxKeys,
  boxStyle,
  hasInherited,
  inherit,
  type TextCtx,
  textNodeStyle,
  textOnlyKeys,
  toPt,
  type Units,
  withStyleProp,
} from './style.ts';
import { svgDataUrl, svgMarkup } from './svg.ts';

/**
 * Page presets in points, portrait: the ISO 216 A and B series, the ISO 269 C series, RA and
 * SRA (ISO 217), the US sizes, Folio and ID-1 (ISO/IEC 7810). Millimetre sizes are rounded to
 * 0.01 pt.
 */
export const PAGE_SIZES: Record<string, { width: number; height: number }> = {
  '4A0': { width: 4767.87, height: 6740.79 },
  '2A0': { width: 3370.39, height: 4767.87 },
  A0: { width: 2383.94, height: 3370.39 },
  A1: { width: 1683.78, height: 2383.94 },
  A2: { width: 1190.55, height: 1683.78 },
  A3: { width: 841.89, height: 1190.55 },
  A4: { width: 595.28, height: 841.89 },
  A5: { width: 419.53, height: 595.28 },
  A6: { width: 297.64, height: 419.53 },
  A7: { width: 209.76, height: 297.64 },
  A8: { width: 147.4, height: 209.76 },
  A9: { width: 104.88, height: 147.4 },
  A10: { width: 73.7, height: 104.88 },
  B0: { width: 2834.65, height: 4008.19 },
  B1: { width: 2004.09, height: 2834.65 },
  B2: { width: 1417.32, height: 2004.09 },
  B3: { width: 1000.63, height: 1417.32 },
  B4: { width: 708.66, height: 1000.63 },
  B5: { width: 498.9, height: 708.66 },
  B6: { width: 354.33, height: 498.9 },
  B7: { width: 249.45, height: 354.33 },
  B8: { width: 175.75, height: 249.45 },
  B9: { width: 124.72, height: 175.75 },
  B10: { width: 87.87, height: 124.72 },
  C0: { width: 2599.37, height: 3676.54 },
  C1: { width: 1836.85, height: 2599.37 },
  C2: { width: 1298.27, height: 1836.85 },
  C3: { width: 918.43, height: 1298.27 },
  C4: { width: 649.13, height: 918.43 },
  C5: { width: 459.21, height: 649.13 },
  C6: { width: 323.15, height: 459.21 },
  C7: { width: 229.61, height: 323.15 },
  C8: { width: 161.57, height: 229.61 },
  C9: { width: 113.39, height: 161.57 },
  C10: { width: 79.37, height: 113.39 },
  RA0: { width: 2437.8, height: 3458.27 },
  RA1: { width: 1729.13, height: 2437.8 },
  RA2: { width: 1218.9, height: 1729.13 },
  RA3: { width: 864.57, height: 1218.9 },
  RA4: { width: 609.45, height: 864.57 },
  SRA0: { width: 2551.18, height: 3628.35 },
  SRA1: { width: 1814.17, height: 2551.18 },
  SRA2: { width: 1275.59, height: 1814.17 },
  SRA3: { width: 907.09, height: 1275.59 },
  SRA4: { width: 637.8, height: 907.09 },
  Letter: { width: 612, height: 792 },
  Legal: { width: 612, height: 1008 },
  Tabloid: { width: 792, height: 1224 },
  Executive: { width: 522, height: 756 },
  Folio: { width: 612, height: 936 },
  ID1: { width: 153.01, height: 242.65 },
};

/** The presets, for error messages. */
const PRESET_NAMES =
  '4A0, 2A0, A0–A10, B0–B10, C0–C10, RA0–RA4, SRA0–SRA4, Letter, Legal, Tabloid, Executive, Folio, ID1';

/** `PAGE_SIZES` by lower-case name: preset names match in any case. */
const PRESETS = new Map(Object.entries(PAGE_SIZES).map(([name, size]) => [name.toLowerCase(), size]));

/** Which remote image URLs `<Image src>` may use; the caller fetches them (`@inkoxide/render` does). */
export interface RemoteImagePolicy {
  /**
   * Allowed URL prefixes (`'https://cdn.example.com/'`) or host names, with `*.` for subdomains
   * (`'*.example.com'`; host entries allow `https:` only).
   */
  allow: readonly string[];
}

/** True when `url` is allowed by the policy. */
export function remoteAllowed(url: string, policy: RemoteImagePolicy | undefined): boolean {
  if (!policy) return false;
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return false;
  }
  if (u.username || u.password) return false;
  return policy.allow.some((entry) => {
    if (entry.includes('://'))
      return u.href.startsWith(entry) && (u.protocol === 'https:' || entry.startsWith('http:'));
    if (u.protocol !== 'https:') return false;
    const host = u.hostname.toLowerCase();
    const e = entry.toLowerCase();
    return e.startsWith('*.') ? host.endsWith(e.slice(1)) && host.length > e.length - 1 : host === e;
  });
}

/**
 * True when an image `src` string is written as a file path: absolute (`/img/logo.png`,
 * `C:\img\logo.png`), relative with `./` or `../`, or a `file:` URL. Other strings are image ids,
 * so a template never reads a file by accident. Paths are files only when the caller allows them
 * (`localImages`); otherwise they are ids too.
 */
export function isImagePath(src: string): boolean {
  return /^(?:\.{0,2}[\\/]|[a-z]:[\\/]|file:)/i.test(src);
}

/**
 * An image the caller loads before rendering (`@inkoxide/render` does): a file, a request
 * allowed by the remote image policy, or a function or promise `src`, not yet called or awaited.
 */
export type PendingImage =
  | { kind: 'file'; path: string }
  | { kind: 'request'; request: ImageRequest }
  | { kind: 'deferred'; src: Promise<ImageSrcValue> | (() => ImageSrcValue | Promise<ImageSrcValue>) };

/**
 * The ids a tree gives the images it cannot refer to by the caller's id: one per bytes, file path,
 * URL, request, function or promise. A tree and its rebuilds share one table, so an image keeps
 * its id on every pass and an id never names two images.
 */
export class ImageIds {
  private readonly ids = new Map<unknown, string>();
  private readonly counts = new Map<string, number>();

  /** The id of `key` (a string, or an object by identity); a new one is `${prefix}-${n}`. */
  of(key: unknown, prefix: string): string {
    let id = this.ids.get(key);
    if (id === undefined) {
      const n = this.counts.get(prefix) ?? 0;
      this.counts.set(prefix, n + 1);
      id = `${prefix}-${n}`;
      this.ids.set(key, id);
    }
    return id;
  }
}

/**
 * What a request sends, as a string: requests that send the same thing share an id however often
 * a template creates them. Undefined for a body that is neither a string nor bytes.
 */
function requestKey(r: ImageRequest): string | undefined {
  const { body } = r;
  const bytes = body instanceof Uint8Array;
  if (body !== undefined && typeof body !== 'string' && !bytes) return undefined;
  const sent = bytes ? Array.from(body, (b) => b.toString(16).padStart(2, '0')).join('') : body;
  const headers = Object.entries(r.headers ?? {})
    .map(([k, v]) => [k.toLowerCase(), String(v)])
    .sort(([a = ''], [b = '']) => (a < b ? -1 : a > b ? 1 : 0));
  return JSON.stringify([r.uri, (r.method ?? 'GET').toUpperCase(), headers, bytes, sent ?? null]);
}

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

const isThenable = (v: unknown): v is PromiseLike<ImageSrcValue> =>
  typeof (v as PromiseLike<unknown> | null)?.then === 'function';

type Inline = Instance | { kind: 'text'; text: string; fiber: unknown };

const isEl = (n: Node | undefined, type: string): n is Instance => n?.kind === 'element' && n.type === type;

function sameColor(a: Color | undefined, b: Color | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3];
}

function transform(text: string, how: TextCtx['textTransform'], lang: string | undefined): string {
  switch (how) {
    case 'uppercase':
      return text.toLocaleUpperCase(lang);
    case 'lowercase':
      return text.toLocaleLowerCase(lang);
    case 'capitalize':
      return text.replace(
        /(^|[\s\-\u2013\u2014(\u201e"\u00ab])(\p{L})/gu,
        (_, pre: string, ch: string) => pre + ch.toLocaleUpperCase(lang),
      );
    default:
      return text;
  }
}

/** A word as a hyphenation callback sees it: letters, their marks, and soft hyphens. */
const WORD = /[\p{L}\p{M}\u00AD]+/gu;
const SHY = '\u00AD';

function toBreak(b: BreakProp | undefined): Break | undefined {
  if (b === undefined) return undefined;
  if (b === 'avoid') return { inside: 'avoid' };
  if (b === 'page-before') return { before: 'page' };
  if (b === 'page-after') return { after: 'page' };
  return b;
}

function toBookmark(b: BookmarkProp | undefined): Bookmark | undefined {
  if (b === undefined) return undefined;
  return typeof b === 'string' ? { title: b } : b;
}

class Builder {
  readonly images: Record<string, Uint8Array> = {};
  private readonly generated: ImageIds;
  private readonly used = new Set<string>();
  readonly warnings: string[] = [];
  private readonly warned = new Set<string>();
  readonly origin: Map<object, Instance> | undefined;
  readonly remote: Record<string, string> = {};
  readonly pending: Record<string, PendingImage> = {};
  private readonly registered: ReadonlySet<string>;
  dynamic = false;
  private docLang: string | undefined;
  /** Hyphenated words by callback, so each distinct word calls it once per build. */
  private readonly hyphenatedWords = new Map<HyphenationCallback, Map<string, string>>();
  /** The size of the page being built: what `vw` and `vh` are relative to. */
  private pageBox = { width: 0, height: 0 };

  constructor(private readonly opts: BuildOptions) {
    this.origin = opts.track ? new Map() : undefined;
    this.registered = new Set(opts.imageIds);
    this.generated = opts.generatedIds ?? new ImageIds();
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
    let s: TwStyle;
    try {
      s = tw(className);
    } catch (e) {
      if (e instanceof UnsupportedUtilityError) {
        this.fail(inst, `${e.message}. In className="${className}" on <${HOST_NAME[inst.type]}>.`, {
          kind: 'tailwind',
          className,
          cause: e,
        });
      }
      throw e;
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
    if (s.objectFit !== undefined && inst.type !== HOST.Image) {
      this.fail(inst, `objectFit applies to <Image>, not <${HOST_NAME[inst.type]}>.`, { kind: 'props' });
    }
    return s;
  }

  private font(ctx: TextCtx, inst: Instance | undefined): string {
    if (ctx.fontId !== undefined) return ctx.fontId;
    const r = this.opts.fonts.resolve(ctx.fontFamily, ctx.fontWeight, ctx.fontStyle);
    ctx.fontId = r.id;
    if (!r.exact && r.wanted && !this.warned.has(r.wanted)) {
      this.warned.add(r.wanted);
      const where = inst
        ? ` (${HOST_NAME[inst.type]}${componentStack(inst).split('\n')[2]?.trim().replace(/^in /, ' in ') ?? ''})`
        : '';
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
    const family = this.opts.defaultFamily ?? tw('font-sans').fontFamily ?? ['Inter'];
    const ctx: TextCtx = {
      fontFamily: family,
      fontWeight: 400,
      fontStyle: 'normal',
      fontSize: 12,
      color: [0, 0, 0, 1],
      lineHeight: undefined,
      letterSpacing: undefined,
      textAlign: undefined,
      textDecoration: undefined,
      textTransform: undefined,
      whiteSpace: undefined,
      hyphenate: undefined,
      decorationColor: undefined,
      decorationThickness: undefined,
      decorationStyle: undefined,
      textIndent: undefined,
      direction: undefined,
      orphans: undefined,
      widows: undefined,
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
    const flow: Node[] = [];
    for (const child of inst.children) {
      if (isEl(child, HOST.Header) || isEl(child, HOST.Footer)) {
        const key = child.type === HOST.Header ? 'header' : 'footer';
        if (spec[key]) this.fail(child, `A <Page> can have only one <${HOST_NAME[child.type]}>.`);
        spec[key] = this.box(
          child,
          ctx,
          child.children,
          bandDefaults(spec.margin, key, this.styleOf(child, ctx)),
        );
      } else {
        flow.push(child);
      }
    }
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
      const [w, h, ...extra] = size as Length[];
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
      const [base = '', orient, ...rest] = size.toLowerCase().split('-');
      const preset = PRESETS.get(base);
      if (preset && rest.length === 0 && (orient === undefined || orient === 'landscape')) {
        return orient ? { width: preset.height, height: preset.width } : { ...preset };
      }
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
      out.push(this.node(n, ctx));
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
    node: { id?: string; break?: Break; link?: string; bookmark?: Bookmark; pages?: PageFilter },
    inst: Instance,
  ): void {
    const p = inst.props;
    if (typeof p.id === 'string') node.id = p.id;
    if (p.pages !== undefined) node.pages = p.pages as PageFilter;
    // Aliases for the same behaviour: `wrap`, `break={true}` and `minPresenceAhead`.
    const br = p.break === true ? 'page-before' : p.break === false ? undefined : p.break;
    let b = toBreak(br as BreakProp | undefined);
    if (p.wrap === false) b = { ...b, inside: 'avoid' };
    if (typeof p.minPresenceAhead === 'number') b = { ...b, minPresenceAhead: p.minPresenceAhead };
    if (b) node.break = b;
    if (p.fixed !== undefined && p.fixed !== false) {
      this.fail(
        inst,
        `\`fixed\` is not supported on <${HOST_NAME[inst.type]}>: put content that repeats on every page in <Header> or <Footer>, which can show <PageNumber /> and <TotalPages />.`,
        { kind: 'props' },
      );
    }
    const href = inst.type === HOST.Link ? this.target(inst) : p.href;
    if (typeof href === 'string') node.link = href;
    const bm = toBookmark(p.bookmark as BookmarkProp | undefined);
    if (bm) node.bookmark = bm;
  }

  /** A `<Link>`'s target: `href`, or its alias `src`. */
  private target(inst: Instance): string {
    const href = inst.props.href ?? inst.props.src;
    if (typeof href !== 'string' || href === '') {
      this.fail(inst, '<Link> needs an href: https://…, mailto:… or #id.', { kind: 'props' });
    }
    return href;
  }

  private node(inst: Instance, ctx: TextCtx): SchemaNode {
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
    else if (typeof inst.props.id === 'string') node.id = inst.props.id;
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

  /** Flattens inline content into runs, merging neighbours with the same resolved style. */
  private runs(nodes: readonly Inline[], ctx: TextCtx, out: TextRun[], owner: Instance): void {
    for (const n of nodes) {
      if (n.kind === 'text') {
        if (n.text === '') continue;
        const text = transform(n.text, ctx.textTransform, ctx.lang);
        // The engine does not break `pre` and `nowrap` text, so the callback is not asked.
        const cb = ctx.whiteSpace === 'pre' || ctx.whiteSpace === 'nowrap' ? undefined : ctx.hyphenation;
        this.push(out, cb ? this.hyphenated(text, cb, owner) : text, ctx, owner, undefined);
        continue;
      }
      switch (n.type) {
        case HOST.Text:
        case HOST.Link: {
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
            n.props.hyphenationCallback !== undefined
              ? inherit(ctx, own, {
                  link: href,
                  lang: n.props.lang as string | undefined,
                  hyphenation: n.props.hyphenationCallback as HyphenationCallback | null | undefined,
                })
              : ctx;
          this.runs(n.children, inner, out, n);
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

  /** Joins the parts `cb` returns for each word with soft hyphens, which the engine breaks at. */
  private hyphenated(text: string, cb: HyphenationCallback, owner: Instance): string {
    let done = this.hyphenatedWords.get(cb);
    if (!done) {
      done = new Map();
      this.hyphenatedWords.set(cb, done);
    }
    const cache = done;
    return text.replace(WORD, (word) => {
      // Soft hyphens already in a word are the author's own break points.
      if (word.includes(SHY)) return word;
      let out = cache.get(word);
      if (out === undefined) {
        out = this.syllablesOf(word, cb, owner).join(SHY);
        cache.set(word, out);
      }
      return out;
    });
  }

  private syllablesOf(word: string, cb: HyphenationCallback, owner: Instance): string[] {
    let parts: unknown;
    try {
      parts = cb(word);
    } catch (e) {
      this.fail(owner, `hyphenationCallback threw for "${word}": ${(e as Error).message}`, {
        kind: 'render',
        cause: e,
      });
    }
    if (!Array.isArray(parts) || parts.some((p) => typeof p !== 'string') || parts.join('') !== word) {
      this.fail(
        owner,
        `hyphenationCallback returned ${JSON.stringify(parts)} for "${word}": return the parts of the word in order (["hy", "phen", "ation"]), or [word] to keep it whole.`,
        { kind: 'props' },
      );
    }
    return (parts as string[]).filter((p) => p !== '');
  }

  private push(
    out: TextRun[],
    text: string | undefined,
    ctx: TextCtx,
    owner: Instance,
    placeholder: TextRun['placeholder'],
  ): void {
    const font = this.font(ctx, owner);
    const size = ctx.fontSize;
    const ls = ctx.letterSpacing;
    const letterSpacing = ls === undefined ? 0 : 'em' in ls ? Math.round(ls.em * size * 1e6) / 1e6 : ls.pt;
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
      last.link === ctx.link
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
    if (typeof p.colSpan === 'number' && p.colSpan !== 1) cell.colSpan = p.colSpan;
    if (typeof p.rowSpan === 'number' && p.rowSpan !== 1) cell.rowSpan = p.rowSpan;
    let style: Style | undefined = boxStyle(own, ctx.color, true);
    if (typeof p.verticalAlign === 'string')
      style = { ...style, verticalAlign: p.verticalAlign as Style['verticalAlign'] };
    if (style) cell.style = style;
    const children = this.children(inst.children, ctx, inst);
    if (children.length > 0) cell.children = children;
    return this.mark(cell, inst);
  }

  private image(inst: Instance, ctx: TextCtx): ImageNode {
    const own = this.styleOf(inst, ctx);
    this.noTextOnly(inst, own);
    const node: ImageNode = { type: 'image' };
    const style = boxStyle(own, ctx.color);
    if (style) node.style = style;
    const src = inst.props.src as ImageSrc | undefined;
    if (src instanceof Uint8Array) {
      node.src = this.generated.of(src, 'image');
      this.images[node.src] = src;
    } else if (typeof src === 'string' && this.registered.has(src)) {
      node.src = src;
    } else if (typeof src === 'string' && src.startsWith('data:')) {
      node.data = src;
    } else if (typeof src === 'string' && this.opts.localImages && isImagePath(src)) {
      node.src = this.pendingId(`file\0${src}`, 'file', () => ({ kind: 'file', path: src }));
    } else if (typeof src === 'string' && /^[a-z][a-z0-9+.-]*:\/\//i.test(src)) {
      this.allowRemote(inst, src, `src="${src}"`);
      node.src = this.generated.of(`remote\0${src}`, 'remote');
      this.remote[node.src] = src;
    } else if (typeof src === 'string' && src.length > 0) {
      node.src = src;
    } else if (typeof src === 'function') {
      node.src = this.pendingId(src, 'deferred', () => ({ kind: 'deferred', src }));
    } else if (isThenable(src)) {
      node.src = this.pendingId(src, 'deferred', () => {
        // Handled now, awaited by the caller: a rejection before then must not crash the process.
        const promise = Promise.resolve(src);
        promise.catch(() => {});
        return { kind: 'deferred', src: promise };
      });
    } else if (typeof src === 'object' && src !== null) {
      if (typeof src.uri !== 'string' || src.uri === '') {
        this.fail(inst, '<Image src={{ uri }}>: a request needs a uri.', { kind: 'props' });
      }
      this.allowRemote(inst, src.uri, `src={{ uri: "${src.uri}" }}`);
      const key = requestKey(src);
      node.src = this.pendingId(key === undefined ? src : `request\0${key}`, 'request', () => ({
        kind: 'request',
        request: src,
      }));
    } else {
      this.fail(
        inst,
        '<Image> needs a src: bytes, a data: URL, an image id, a file path, a URL, a request, or a function or promise.',
        { kind: 'props' },
      );
    }
    const fit = (inst.props.fit as ImageNode['fit']) ?? own.objectFit;
    if (fit) node.fit = fit;
    this.flowProps(node, inst);
    return this.mark(node, inst);
  }

  private allowRemote(inst: Instance, url: string, prop: string): void {
    if (remoteAllowed(url, this.opts.remoteImages)) return;
    this.fail(
      inst,
      `<Image ${prop}>: this URL is not in the remoteImages allowlist. Add its origin to the render option remoteImages.allow (or the INKOXIDE_REMOTE_IMAGES variable), or pass the bytes.`,
      { kind: 'props' },
    );
  }

  /** One id per file path, request, function or promise, however many images use it. */
  private pendingId(key: unknown, prefix: string, image: () => PendingImage): string {
    const id = this.generated.of(key, prefix);
    this.pending[id] ??= image();
    return id;
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
    const n = node as { id?: string; break?: Break; pages?: PageFilter };
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
      markup = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${painter.toSvgBody()}</svg>`;
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
    // Several variants: the element's place in the layout (outer box) wraps them.
    const wrapper: SchemaNode = { type: 'box', children: [] };
    const outer: Style = { display: 'grid' };
    for (const { node, pages } of all) {
      const v = node as SchemaNode & Record<string, unknown>;
      for (const k of ['id', 'break', 'link', 'bookmark', 'pages'] as const) {
        if (v[k] !== undefined) (wrapper as unknown as Record<string, unknown>)[k] = v[k];
        delete v[k];
      }
      const inner: Record<string, unknown> = { ...(v.style as Record<string, unknown> | undefined) };
      for (const k of OUTER_KEYS) {
        if (inner[k] !== undefined) (outer as Record<string, unknown>)[k] = inner[k];
        delete inner[k];
      }
      inner.gridRow = { start: 1 };
      inner.gridColumn = { start: 1 };
      v.style = inner as Style;
      // An own `pages` filter on the element narrows every variant.
      v.pages = pages as [number, ...number[]];
      wrapper.children?.push(v);
    }
    wrapper.style = outer;
    return this.mark(wrapper, inst);
  }
}

/** Style keys that place an element among its siblings (kept on a per-page wrapper). */
const OUTER_KEYS = [
  'margin',
  'inset',
  'position',
  'flexGrow',
  'flexShrink',
  'flexBasis',
  'alignSelf',
  'justifySelf',
  'gridColumn',
  'gridRow',
  'width',
  'height',
  'minWidth',
  'minHeight',
  'maxWidth',
  'maxHeight',
  'aspectRatio',
  'zIndex',
  'rotate',
  'transform',
  'transformOrigin',
] as const;

/** Builds the schema document from rendered host instances. */
export function buildTree(container: Container, opts: BuildOptions): BuildResult {
  const b = new Builder(opts);
  const document = b.build(container);
  return {
    document,
    images: b.images,
    fonts: b.fontIds(),
    warnings: b.warnings,
    origin: b.origin,
    dynamic: b.dynamic,
    remote: b.remote,
    pending: b.pending,
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
