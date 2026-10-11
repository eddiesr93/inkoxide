/**
 * A pdfkit-style painter for `<Canvas paint={…}>`. It records SVG: every `fill()` / `stroke()`
 * emits the current path with the current state, and the drawing becomes an SVG image.
 * Coordinates are points, origin top left, y down.
 */

const fmt = (n: number): string => String(Math.round(n * 1000) / 1000);

export const escapeXml = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

interface State {
  fill: string;
  stroke: string;
  fillOpacity: number;
  strokeOpacity: number;
  opacity: number;
  lineWidth: number;
  lineCap: 'butt' | 'round' | 'square';
  lineJoin: 'miter' | 'round' | 'bevel';
  dash: number[] | undefined;
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
  dash: undefined,
};

export class Painter {
  private d = '';
  private state: State = { ...INITIAL };
  private readonly stack: Array<{ state: State; groups: number }> = [];
  /** Open `<g transform>` elements since the last save(). */
  private groups = 0;
  private out = '';

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

  fillColor(color: string, opacity?: number): this {
    this.state.fill = color;
    if (opacity !== undefined) this.state.fillOpacity = opacity;
    return this;
  }

  strokeColor(color: string, opacity?: number): this {
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

  // Painting --------------------------------------------------------------------------------------

  fill(color?: string, rule: 'nonzero' | 'evenodd' = 'nonzero'): this {
    if (color !== undefined) this.state.fill = color;
    return this.emit(true, false, rule);
  }

  stroke(color?: string): this {
    if (color !== undefined) this.state.stroke = color;
    return this.emit(false, true, 'nonzero');
  }

  fillAndStroke(fill?: string, stroke: string | undefined = fill): this {
    if (fill !== undefined) this.state.fill = fill;
    if (stroke !== undefined) this.state.stroke = stroke;
    return this.emit(true, true, 'nonzero');
  }

  private emit(fill: boolean, stroke: boolean, rule: 'nonzero' | 'evenodd'): this {
    if (this.d === '') return this;
    const s = this.state;
    let a = `<path d="${escapeXml(this.d)}" fill="${fill ? escapeXml(s.fill) : 'none'}"`;
    if (fill && s.fillOpacity !== 1) a += ` fill-opacity="${fmt(s.fillOpacity)}"`;
    if (fill && rule === 'evenodd') a += ' fill-rule="evenodd"';
    if (stroke) {
      a += ` stroke="${escapeXml(s.stroke)}" stroke-width="${fmt(s.lineWidth)}"`;
      if (s.strokeOpacity !== 1) a += ` stroke-opacity="${fmt(s.strokeOpacity)}"`;
      if (s.lineCap !== 'butt') a += ` stroke-linecap="${s.lineCap}"`;
      if (s.lineJoin !== 'miter') a += ` stroke-linejoin="${s.lineJoin}"`;
      if (s.dash) a += ` stroke-dasharray="${s.dash.map(fmt).join(' ')}"`;
    }
    if (s.opacity !== 1) a += ` opacity="${fmt(s.opacity)}"`;
    this.out += `${a}/>`;
    this.d = '';
    return this;
  }

  /** The recorded drawing, every open group closed. */
  toSvgBody(): string {
    let out = this.out + '</g>'.repeat(this.groups);
    for (let i = this.stack.length - 1; i >= 0; i--) out += '</g>'.repeat(this.stack[i]?.groups ?? 0);
    return out;
  }
}
