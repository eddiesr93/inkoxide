/**
 * A pdfkit-style painter for `<Canvas paint={…}>`. It records SVG: every `fill()` / `stroke()`
 * emits the current path with the current state, and the drawing becomes an SVG image.
 * Coordinates are points, origin top left, y down.
 */

const fmt = (n: number): string => String(Math.round(n * 1000) / 1000);

export const escapeXml = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** What a `CanvasGradient` writes: its element, its geometry attributes and its stops so far. */
interface GradientDef {
  tag: 'linearGradient' | 'radialGradient';
  geometry: string;
  stops: string[];
}
// Kept out of the class, so a gradient's public API is `stop()` alone.
const defs = new WeakMap<CanvasGradient, GradientDef>();

/**
 * A gradient from `painter.linearGradient(…)` or `painter.radialGradient(…)`, for `fill`, `stroke`,
 * `fillAndStroke`, `fillColor` and `strokeColor`. Its coordinates are in the user space of the
 * shape it paints, at the time it is painted, as in pdfkit.
 */
export class CanvasGradient {
  /** Use `painter.linearGradient` or `painter.radialGradient`. */
  private constructor() {}

  /** @internal */
  static create(tag: GradientDef['tag'], geometry: string): CanvasGradient {
    const g = new CanvasGradient();
    defs.set(g, { tag, geometry, stops: [] });
    return g;
  }

  /** Adds a colour stop at `offset` (0 to 1), with an opacity from 0 to 1. */
  stop(offset: number, color: string, opacity?: number): this {
    let s = `<stop offset="${fmt(offset)}" stop-color="${escapeXml(color)}"`;
    if (opacity !== undefined) s += ` stop-opacity="${fmt(opacity)}"`;
    defs.get(this)?.stops.push(`${s}/>`);
    return this;
  }
}

/** A gradient's element with the id `id`, as it is now. */
function gradientMarkup(g: CanvasGradient, id: string): string {
  const d = defs.get(g);
  if (!d) {
    throw new TypeError(
      'A paint is a colour string, or a gradient from painter.linearGradient or painter.radialGradient.',
    );
  }
  return `<${d.tag} id="${id}" gradientUnits="userSpaceOnUse"${d.geometry}>${d.stops.join('')}</${d.tag}>`;
}

type Paint = string | CanvasGradient;

/** A font as the text of a drawing (`<Svg>`, `<Canvas>`) asks for it: a family stack, a weight and a style. */
export interface DrawingFont {
  family: readonly string[];
  weight: number;
  style: 'normal' | 'italic';
}

/** A family name for an SVG `font-family`, quoted. */
export const quotedFamily = (family: string): string => `'${family.replace(/['\\]/g, (c) => `\\${c}`)}'`;

/** What the painter's text needs from the tree builder. */
export interface PainterFonts {
  /** The text style around the canvas: the font until `font()` sets one. */
  font: DrawingFont;
  /** The face of registered font id `id`, if it is one. */
  byId(id: string): DrawingFont | undefined;
  /** The registered face a font resolves to, which the document then declares. */
  face(font: DrawingFont): DrawingFont;
}

/** Options of `painter.text`. */
export interface CanvasTextOptions {
  /** With `align`, the width the line is aligned in, from `x`. */
  width?: number;
  align?: 'left' | 'center' | 'right' | 'justify';
  /** Which line of the text is at `y`: `top` (default), `middle`, `bottom`, `alphabetic`, `hanging`. */
  baseline?: 'top' | 'middle' | 'bottom' | 'alphabetic' | 'hanging';
  /** Extra space after each character, in points. */
  characterSpacing?: number;
  /** Extra space after each space, in points. */
  wordSpacing?: number;
  underline?: boolean;
  strike?: boolean;
  /** Fill the text with the fill colour (default true). */
  fill?: boolean;
  /** Stroke the text with the stroke colour and line style (default false). */
  stroke?: boolean;
}

/** pdfkit's standard font names: the canvas's family, with their weight and style. */
const STANDARD_FONTS: Readonly<Record<string, [number, 'normal' | 'italic']>> = {
  Helvetica: [400, 'normal'],
  'Helvetica-Bold': [700, 'normal'],
  'Helvetica-Oblique': [400, 'italic'],
  'Helvetica-BoldOblique': [700, 'italic'],
  'Times-Roman': [400, 'normal'],
  'Times-Bold': [700, 'normal'],
  'Times-Italic': [400, 'italic'],
  'Times-BoldItalic': [700, 'italic'],
  Courier: [400, 'normal'],
  'Courier-Bold': [700, 'normal'],
  'Courier-Oblique': [400, 'italic'],
  'Courier-BoldOblique': [700, 'italic'],
  Symbol: [400, 'normal'],
  ZapfDingbats: [400, 'normal'],
};

const BASELINES: Readonly<Record<NonNullable<CanvasTextOptions['baseline']>, string>> = {
  top: 'text-before-edge',
  middle: 'middle',
  bottom: 'text-after-edge',
  alphabetic: 'alphabetic',
  hanging: 'hanging',
};

interface State {
  fill: Paint;
  stroke: Paint;
  fillOpacity: number;
  strokeOpacity: number;
  opacity: number;
  lineWidth: number;
  lineCap: 'butt' | 'round' | 'square';
  lineJoin: 'miter' | 'round' | 'bevel';
  miterLimit: number;
  dash: number[] | undefined;
  /** Set by `font()`; the canvas's text style until then. */
  font: DrawingFont | undefined;
  fontSize: number;
}

const INITIAL: State = {
  fill: 'black',
  stroke: 'black',
  fillOpacity: 1,
  strokeOpacity: 1,
  opacity: 1,
  lineWidth: 1,
  lineCap: 'butt',
  lineJoin: 'miter',
  // PDF's default, as in pdfkit; SVG's is 4.
  miterLimit: 10,
  dash: undefined,
  font: undefined,
  // pdfkit's default.
  fontSize: 12,
};

export class Painter {
  /** @internal The tree builder makes the painter. */
  constructor(private readonly fonts: PainterFonts) {}

  private d = '';
  private state: State = { ...INITIAL };
  private readonly stack: Array<{ state: State; groups: number }> = [];
  /** Open `<g transform>` and `<g clip-path>` elements since the last save(). */
  private groups = 0;
  private out = '';
  /** Clip paths written so far, for their ids. */
  private clips = 0;
  /** The id of each gradient written so far, by its markup without the id. */
  private readonly gradients = new Map<string, string>();

  // Path construction ---------------------------------------------------------------------------

  moveTo(x: number, y: number): this {
    this.d += `M${fmt(x)} ${fmt(y)}`;
    return this;
  }

  lineTo(x: number, y: number): this {
    this.d += `L${fmt(x)} ${fmt(y)}`;
    return this;
  }

  bezierCurveTo(c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number): this {
    this.d += `C${[c1x, c1y, c2x, c2y, x, y].map(fmt).join(' ')}`;
    return this;
  }

  quadraticCurveTo(cx: number, cy: number, x: number, y: number): this {
    this.d += `Q${[cx, cy, x, y].map(fmt).join(' ')}`;
    return this;
  }

  closePath(): this {
    this.d += 'Z';
    return this;
  }

  rect(x: number, y: number, w: number, h: number): this {
    this.d += `M${fmt(x)} ${fmt(y)}h${fmt(w)}v${fmt(h)}h${fmt(-w)}Z`;
    return this;
  }

  roundedRect(x: number, y: number, w: number, h: number, r: number): this {
    const k = Math.max(0, Math.min(r, w / 2, h / 2));
    if (k === 0) return this.rect(x, y, w, h);
    this.d +=
      `M${fmt(x + k)} ${fmt(y)}H${fmt(x + w - k)}A${fmt(k)} ${fmt(k)} 0 0 1 ${fmt(x + w)} ${fmt(y + k)}` +
      `V${fmt(y + h - k)}A${fmt(k)} ${fmt(k)} 0 0 1 ${fmt(x + w - k)} ${fmt(y + h)}` +
      `H${fmt(x + k)}A${fmt(k)} ${fmt(k)} 0 0 1 ${fmt(x)} ${fmt(y + h - k)}` +
      `V${fmt(y + k)}A${fmt(k)} ${fmt(k)} 0 0 1 ${fmt(x + k)} ${fmt(y)}Z`;
    return this;
  }

  ellipse(cx: number, cy: number, rx: number, ry: number = rx): this {
    this.d +=
      `M${fmt(cx - rx)} ${fmt(cy)}A${fmt(rx)} ${fmt(ry)} 0 1 0 ${fmt(cx + rx)} ${fmt(cy)}` +
      `A${fmt(rx)} ${fmt(ry)} 0 1 0 ${fmt(cx - rx)} ${fmt(cy)}Z`;
    return this;
  }

  circle(cx: number, cy: number, r: number): this {
    return this.ellipse(cx, cy, r, r);
  }

  polygon(...points: Array<[number, number]>): this {
    points.forEach(([x, y], i) => {
      this.d += `${i === 0 ? 'M' : 'L'}${fmt(x)} ${fmt(y)}`;
    });
    if (points.length > 0) this.d += 'Z';
    return this;
  }

  /** Appends SVG path data. */
  path(d: string): this {
    this.d += d;
    return this;
  }

  // State -----------------------------------------------------------------------------------------

  fillColor(color: Paint, opacity?: number): this {
    this.state.fill = color;
    if (opacity !== undefined) this.state.fillOpacity = opacity;
    return this;
  }

  strokeColor(color: Paint, opacity?: number): this {
    this.state.stroke = color;
    if (opacity !== undefined) this.state.strokeOpacity = opacity;
    return this;
  }

  fillOpacity(o: number): this {
    this.state.fillOpacity = o;
    return this;
  }

  strokeOpacity(o: number): this {
    this.state.strokeOpacity = o;
    return this;
  }

  opacity(o: number): this {
    this.state.opacity = o;
    return this;
  }

  lineWidth(w: number): this {
    this.state.lineWidth = w;
    return this;
  }

  lineCap(c: State['lineCap']): this {
    this.state.lineCap = c;
    return this;
  }

  lineJoin(j: State['lineJoin']): this {
    this.state.lineJoin = j;
    return this;
  }

  /** How far a mitred join may reach, as a multiple of the line width, before it is bevelled. */
  miterLimit(m: number): this {
    this.state.miterLimit = m;
    return this;
  }

  dash(length: number, opts: { space?: number } = {}): this {
    this.state.dash = [length, opts.space ?? length];
    return this;
  }

  undash(): this {
    this.state.dash = undefined;
    return this;
  }

  save(): this {
    this.stack.push({ state: { ...this.state }, groups: this.groups });
    this.groups = 0;
    return this;
  }

  restore(): this {
    const top = this.stack.pop();
    if (!top) return this;
    this.out += '</g>'.repeat(this.groups);
    this.state = top.state;
    this.groups = top.groups;
    return this;
  }

  // Transforms apply to what is painted after them, until restore().

  translate(x: number, y: number): this {
    return this.group(`translate(${fmt(x)} ${fmt(y)})`);
  }

  rotate(deg: number, opts: { origin?: [number, number] } = {}): this {
    const [ox, oy] = opts.origin ?? [0, 0];
    return this.group(`rotate(${fmt(deg)} ${fmt(ox)} ${fmt(oy)})`);
  }

  scale(x: number, y: number = x): this {
    return this.group(`scale(${fmt(x)} ${fmt(y)})`);
  }

  private group(transform: string): this {
    this.out += `<g transform="${transform}">`;
    this.groups++;
    return this;
  }

  // Clipping and gradients ----------------------------------------------------------------------

  /** Clips what is painted after it to the current path, until restore(). Ends the path. */
  clip(rule: 'nonzero' | 'evenodd' = 'nonzero'): this {
    const id = `ink-c${++this.clips}`;
    const path =
      this.d === ''
        ? ''
        : `<path d="${escapeXml(this.d)}"${rule === 'evenodd' ? ' clip-rule="evenodd"' : ''}/>`;
    this.out += `<clipPath id="${id}">${path}</clipPath><g clip-path="url(#${id})">`;
    this.groups++;
    this.d = '';
    return this;
  }

  /** A linear gradient from (`x1`, `y1`) to (`x2`, `y2`); add colours with `stop()`. */
  linearGradient(x1: number, y1: number, x2: number, y2: number): CanvasGradient {
    return CanvasGradient.create(
      'linearGradient',
      ` x1="${fmt(x1)}" y1="${fmt(y1)}" x2="${fmt(x2)}" y2="${fmt(y2)}"`,
    );
  }

  /**
   * A radial gradient from the circle at (`x1`, `y1`) with radius `r1` to the circle at (`x2`,
   * `y2`) with radius `r2`; add colours with `stop()`.
   */
  radialGradient(x1: number, y1: number, r1: number, x2: number, y2: number, r2: number): CanvasGradient {
    return CanvasGradient.create(
      'radialGradient',
      ` fx="${fmt(x1)}" fy="${fmt(y1)}" fr="${fmt(r1)}" cx="${fmt(x2)}" cy="${fmt(y2)}" r="${fmt(r2)}"`,
    );
  }

  /**
   * Writes what paint `p` needs (a gradient's element, the first time) and returns the value of a
   * `fill` or `stroke` attribute that refers to it.
   */
  private writePaint(p: Paint): string {
    if (typeof p === 'string') return escapeXml(p);
    const key = gradientMarkup(p, '');
    let id = this.gradients.get(key);
    if (id === undefined) {
      id = `ink-g${this.gradients.size + 1}`;
      this.gradients.set(key, id);
      this.out += `<defs>${gradientMarkup(p, id)}</defs>`;
    }
    return `url(#${id})`;
  }

  // Painting --------------------------------------------------------------------------------------

  fill(color?: Paint, rule: 'nonzero' | 'evenodd' = 'nonzero'): this {
    if (color !== undefined) this.state.fill = color;
    return this.emit(true, false, rule);
  }

  stroke(color?: Paint): this {
    if (color !== undefined) this.state.stroke = color;
    return this.emit(false, true, 'nonzero');
  }

  fillAndStroke(fill?: Paint, stroke: Paint | undefined = fill): this {
    if (fill !== undefined) this.state.fill = fill;
    if (stroke !== undefined) this.state.stroke = stroke;
    return this.emit(true, true, 'nonzero');
  }

  private emit(fill: boolean, stroke: boolean, rule: 'nonzero' | 'evenodd'): this {
    if (this.d === '') return this;
    const a = `<path d="${escapeXml(this.d)}"${this.paintAttrs(fill, stroke, rule)}`;
    this.out += `${a}/>`;
    this.d = '';
    return this;
  }

  /** The fill, stroke and opacity attributes of what is painted next, in the current state. */
  private paintAttrs(fill: boolean, stroke: boolean, rule: 'nonzero' | 'evenodd'): string {
    const s = this.state;
    // Gradients are written before the shape that uses them.
    const fillPaint = fill ? this.writePaint(s.fill) : 'none';
    const strokePaint = stroke ? this.writePaint(s.stroke) : '';
    let a = ` fill="${fillPaint}"`;
    if (fill && s.fillOpacity !== 1) a += ` fill-opacity="${fmt(s.fillOpacity)}"`;
    if (fill && rule === 'evenodd') a += ' fill-rule="evenodd"';
    if (stroke) {
      a += ` stroke="${strokePaint}" stroke-width="${fmt(s.lineWidth)}"`;
      if (s.strokeOpacity !== 1) a += ` stroke-opacity="${fmt(s.strokeOpacity)}"`;
      if (s.lineCap !== 'butt') a += ` stroke-linecap="${s.lineCap}"`;
      if (s.lineJoin === 'miter') a += ` stroke-miterlimit="${fmt(s.miterLimit)}"`;
      else a += ` stroke-linejoin="${s.lineJoin}"`;
      if (s.dash) a += ` stroke-dasharray="${s.dash.map(fmt).join(' ')}"`;
    }
    if (s.opacity !== 1) a += ` opacity="${fmt(s.opacity)}"`;
    return a;
  }

  // Text ----------------------------------------------------------------------------------------

  /**
   * The font of the text after it: a registered font id (`Inter-Bold`), a family name, or one of
   * pdfkit's standard names (`Helvetica-Bold`), which pick the canvas's family in their weight
   * and style. Optionally the size too.
   */
  font(name: string, size?: number): this {
    const standard = STANDARD_FONTS[name];
    const around = this.fonts.font.family;
    this.state.font =
      this.fonts.byId(name) ??
      (standard
        ? { family: [name.split('-')[0] ?? name, ...around], weight: standard[0], style: standard[1] }
        : { family: [name], weight: 400, style: 'normal' });
    if (size !== undefined) this.state.fontSize = size;
    return this;
  }

  fontSize(size: number): this {
    this.state.fontSize = size;
    return this;
  }

  /**
   * Draws one line of text at (`x`, `y`), its top there by default (`baseline`), with the fill
   * colour (or gradient) and, with `stroke`, the stroke. There is no wrapping: a line break is an
   * error.
   */
  text(text: string, x: number, y: number, opts: CanvasTextOptions = {}): this {
    if (/[\r\n]/.test(text)) {
      throw new Error('painter.text draws one line; call it once per line.');
    }
    const s = this.state;
    const face = this.fonts.face(s.font ?? this.fonts.font);
    const width = opts.width ?? 0;
    const [at, anchor] =
      opts.align === 'center'
        ? [x + width / 2, 'middle']
        : opts.align === 'right'
          ? [x + width, 'end']
          : [x, 'start'];
    const family = quotedFamily(face.family[0] ?? '');
    let a = `<text x="${fmt(at)}" y="${fmt(y)}" xml:space="preserve" font-family="${escapeXml(family)}"`;
    a += ` font-weight="${face.weight}"`;
    if (face.style === 'italic') a += ' font-style="italic"';
    a += ` font-size="${fmt(s.fontSize)}"`;
    if (anchor !== 'start') a += ` text-anchor="${anchor}"`;
    a += ` dominant-baseline="${BASELINES[opts.baseline ?? 'top']}"`;
    if (opts.characterSpacing) a += ` letter-spacing="${fmt(opts.characterSpacing)}"`;
    if (opts.wordSpacing) a += ` word-spacing="${fmt(opts.wordSpacing)}"`;
    const lines = [opts.underline && 'underline', opts.strike && 'line-through'].filter(Boolean);
    if (lines.length > 0) a += ` text-decoration="${lines.join(' ')}"`;
    a += this.paintAttrs(opts.fill !== false, opts.stroke === true, 'nonzero');
    this.out += `${a}>${escapeXml(text)}</text>`;
    return this;
  }

  /** The recorded drawing, every open group closed. */
  toSvgBody(): string {
    let out = this.out + '</g>'.repeat(this.groups);
    for (let i = this.stack.length - 1; i >= 0; i--) out += '</g>'.repeat(this.stack[i]?.groups ?? 0);
    return out;
  }
}
