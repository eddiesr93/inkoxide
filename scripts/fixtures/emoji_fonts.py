#!/usr/bin/env python3
"""Generates the colour emoji test fonts, written from scratch (no font
tools, no third-party artwork), so they are small and free of licences:

- fixtures/fonts/EmojiTest-COLR.ttf: TrueType outlines plus COLR v0 and v1
  colour glyphs (solid fills, linear, radial and sweep gradients,
  transforms, composites in several modes, a glyph reused through PaintColrGlyph, clip
  boxes, a layer in the foreground colour), and GSUB ligatures for a ZWJ
  sequence, a flag, a keycap and a skin tone;
- fixtures/fonts/EmojiTest-CBDT.ttf: no outlines, PNG bitmaps in CBDT, and
  the same ligatures for the flag;
- fixtures/fonts/EmojiTest-sbix.ttf: no outlines, PNG bitmaps in sbix, in
  two strikes (32 and 64 ppem);
- fixtures/fonts/EmojiTest-SVG.ttf: TrueType outlines plus an SVG table:
  a compressed document of one glyph with a root viewBox, a document of
  two glyphs with shared definitions (one in the text colour), and one
  whose glyphs are nested in a group;
- fixtures/fonts/limits/colr-fanout.ttf: one COLR v1 glyph whose layers
  each paint the next level's glyph, 255 times per level, eight levels
  deep (about 10^19 paints): the painter's work budget must stop it;
- fixtures/fonts/limits/colr-fanout-empty.ttf: the same, but the last
  level paints nothing, so the painter is never called: counting the paint
  graph before painting must stop it.

The output is committed; scripts/check-generated.sh regenerates it and
fails when it differs, so edit this script, not the fonts.

  scripts/fixtures/emoji_fonts.py
"""
import gzip
import math
import pathlib
import struct
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
FONTS = ROOT / "fixtures" / "fonts"

UPEM = 1000
ASCENT = 900
DESCENT = -200
EMOJI_ADVANCE = 1100
# The square emoji are drawn in: x 50..1050, y -150..850.
BOX = (50, -150, 1050, 850)


# --- Outlines ---------------------------------------------------------------

def rect(x0, y0, x1, y1):
    """A clockwise rectangle contour: [(x, y, on_curve)]."""
    return [(x0, y0, True), (x0, y1, True), (x1, y1, True), (x1, y0, True)]


def circle(cx, cy, r):
    """A clockwise circle of eight quadratic arcs."""
    pts = []
    k = r / math.cos(math.pi / 8)
    for i in range(8):
        a = -i * math.pi / 4
        pts.append((round(cx + r * math.cos(a)), round(cy + r * math.sin(a)), True))
        b = a - math.pi / 8
        pts.append((round(cx + k * math.cos(b)), round(cy + k * math.sin(b)), False))
    return pts


def diamond(cx, cy, rx, ry):
    return [(cx, cy - ry, True), (cx - rx, cy, True), (cx, cy + ry, True), (cx + rx, cy, True)]


class Glyph:
    def __init__(self, name, contours=(), advance=EMOJI_ADVANCE):
        self.name = name
        self.contours = [list(c) for c in contours]
        self.advance = advance

    def bounds(self):
        pts = [p for c in self.contours for p in c]
        if not pts:
            return (0, 0, 0, 0)
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        return (min(xs), min(ys), max(xs), max(ys))


def glyf_data(g):
    if not g.contours:
        return b""
    x0, y0, x1, y1 = g.bounds()
    out = struct.pack(">hhhhh", len(g.contours), x0, y0, x1, y1)
    end = -1
    for c in g.contours:
        end += len(c)
        out += struct.pack(">H", end)
    out += struct.pack(">H", 0)  # no instructions
    pts = [p for c in g.contours for p in c]
    out += bytes(1 if on else 0 for _, _, on in pts)
    px = py = 0
    xs = ys = b""
    for x, y, _ in pts:
        xs += struct.pack(">h", x - px)
        ys += struct.pack(">h", y - py)
        px, py = x, y
    out += xs + ys
    return out + b"\0" * (-len(out) % 4)


# --- Tables -----------------------------------------------------------------

def head(bounds, loca_long=True):
    x0, y0, x1, y1 = bounds
    return struct.pack(
        ">IIIIHHqqhhhhHHhhh",
        0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0b11, UPEM, 0, 0,
        x0, y0, x1, y1, 0, 8, 2, 1 if loca_long else 0, 0,
    )


def hhea(glyphs):
    return struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00010000, ASCENT, DESCENT, 0, max(g.advance for g in glyphs),
        0, 0, EMOJI_ADVANCE, 1, 0, 0, 0, 0, 0, 0, 0, len(glyphs),
    )


def maxp(glyphs, outlines):
    if not outlines:
        return struct.pack(">IH", 0x00005000, len(glyphs))
    points = max([sum(len(c) for c in g.contours) for g in glyphs] + [0])
    contours = max([len(g.contours) for g in glyphs] + [0])
    return struct.pack(
        ">IHHHHHHHHHHHHHH", 0x00010000, len(glyphs), points, contours,
        0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0,
    )


def hmtx(glyphs):
    return b"".join(struct.pack(">Hh", g.advance, g.bounds()[0]) for g in glyphs)


def cmap(mapping):
    """Format 12 for every code point, under Unicode full and Windows UCS-4."""
    groups = []
    for cp, gid in sorted(mapping.items()):
        if groups and groups[-1][1] + 1 == cp and groups[-1][2] + (cp - groups[-1][0]) == gid:
            groups[-1][1] = cp
        else:
            groups.append([cp, cp, gid])
    sub = struct.pack(">HHIII", 12, 0, 16 + 12 * len(groups), 0, len(groups))
    sub += b"".join(struct.pack(">III", a, b, g) for a, b, g in groups)
    return struct.pack(">HHHHIHHI", 0, 2, 0, 4, 20, 3, 10, 20) + sub


def name(family):
    records = [(1, family), (2, "Regular"), (4, family + " Regular"), (6, family.replace(" ", "") + "-Regular")]
    data = b""
    entries = b""
    for nid, text in records:
        s = text.encode("utf-16-be")
        entries += struct.pack(">HHHHHH", 3, 1, 0x409, nid, len(s), len(data))
        data += s
    return struct.pack(">HHH", 0, len(records), 6 + 12 * len(records)) + entries + data


def os2():
    return struct.pack(
        ">HhHHHhhhhhhhhhhh10sIIIIIHHHhhhHHIIhhHHH",
        4, 600, 400, 5, 0, 650, 600, 0, 75, 650, 600, 0, 350, 50, 250, 0,
        b"\0" * 10, 0, 0, 0, 0, 0, 0x40, 0x20, 0xFFFF, ASCENT, DESCENT, 0,
        ASCENT, -DESCENT, 1, 0, 500, 700, 0, 32, 0,
    )


def post():
    return struct.pack(">IIhhIIIII", 0x00030000, 0, -100, 50, 0, 0, 0, 0, 0)


def coverage(gids):
    gids = sorted(gids)
    return struct.pack(">HH", 1, len(gids)) + b"".join(struct.pack(">H", g) for g in gids)


def gsub(ligatures):
    """`ccmp` in the default script: one ligature lookup. `ligatures`:
    [([first, second, ...], ligature)]."""
    by_first = {}
    for comps, lig in ligatures:
        by_first.setdefault(comps[0], []).append((comps, lig))
    firsts = sorted(by_first)
    # LigatureSubst format 1.
    sets = []
    for f in firsts:
        ligs = sorted(by_first[f], key=lambda x: -len(x[0]))
        body = b""
        offs = []
        base = 2 + 2 * len(ligs)
        for comps, lig in ligs:
            offs.append(base + len(body))
            body += struct.pack(">HH", lig, len(comps)) + b"".join(struct.pack(">H", c) for c in comps[1:])
        sets.append(struct.pack(">H", len(ligs)) + b"".join(struct.pack(">H", o) for o in offs) + body)
    head_len = 6 + 2 * len(sets)
    cov = coverage(firsts)
    sets_blob = b""
    set_offs = []
    for s in sets:
        set_offs.append(head_len + len(cov) + len(sets_blob))
        sets_blob += s
    subtable = struct.pack(">HHH", 1, head_len, len(sets)) + b"".join(struct.pack(">H", o) for o in set_offs)
    subtable += cov + sets_blob
    lookup = struct.pack(">HHHH", 4, 0, 1, 8) + subtable
    lookup_list = struct.pack(">HH", 1, 4) + lookup
    feature = struct.pack(">HHH", 0, 1, 0)
    feature_list = struct.pack(">H4sH", 1, b"ccmp", 8) + feature
    langsys = struct.pack(">HHHH", 0, 0xFFFF, 1, 0)
    script = struct.pack(">HH", 4, 0) + langsys
    script_list = struct.pack(">H4sH", 1, b"DFLT", 8) + script
    off_script = 10
    off_feature = off_script + len(script_list)
    off_lookup = off_feature + len(feature_list)
    return struct.pack(">IHHH", 0x00010000, off_script, off_feature, off_lookup) + script_list + feature_list + lookup_list


def cpal(colors):
    """One palette of (r, g, b, a) bytes."""
    recs = b"".join(struct.pack(">BBBB", b, g, r, a) for r, g, b, a in colors)
    return struct.pack(">HHHHIH", 0, len(colors), 1, len(colors), 14, 0) + recs


# COLR v1 paints, as tuples serialised by `emit`.
def solid(i, alpha=1.0):
    return ("solid", i, alpha)


def stops(*pairs):
    return list(pairs)


def colrv1(v0, v1, clips, layers=None, var_map=b"", var_store=b""):
    """`v0`: {gid: [(layer gid, palette index)]}; `v1`: {gid: paint};
    `clips`: {gid: (x0, y0, x1, y1)}; `layers`: the paints of the layer list;
    `var_map` and `var_store`: the delta-set index map and the item
    variation store of a variable font's paints."""
    layers = layers or []

    def f2dot14(v):
        return struct.pack(">h", round(v * 16384))

    def fixed(v):
        return struct.pack(">i", round(v * 65536))

    def colorline(extend, st):
        out = struct.pack(">BH", extend, len(st))
        for off, idx, alpha in st:
            out += f2dot14(off) + struct.pack(">H", idx) + f2dot14(alpha)
        return out

    def emit(p, buf):
        """Writes paint `p` at the end of `buf` (children after it); returns its position."""
        at = len(buf)
        kind = p[0]
        if kind == "layers":  # PaintColrLayers
            buf += struct.pack(">BBI", 1, p[2], p[1])
        elif kind == "solid":
            buf += struct.pack(">BH", 2, p[1]) + f2dot14(p[2])
        elif kind == "varsolid":  # (varsolid, palette index, alpha, var index base)
            buf += struct.pack(">BH", 3, p[1]) + f2dot14(p[2]) + struct.pack(">I", p[3])
        elif kind == "linear":  # (linear, p0, p1, p2, stops)
            _, (x0, y0), (x1, y1), (x2, y2), st = p
            buf += struct.pack(">B", 4) + b"\0\0\0" + struct.pack(">hhhhhh", x0, y0, x1, y1, x2, y2)
            line = len(buf)
            buf += colorline(0, st)
            buf[at + 1:at + 4] = (line - at).to_bytes(3, "big")
        elif kind == "radial":  # (radial, (x0, y0, r0), (x1, y1, r1), stops)
            _, (x0, y0, r0), (x1, y1, r1), st = p
            buf += struct.pack(">B", 6) + b"\0\0\0" + struct.pack(">hhHhhH", x0, y0, r0, x1, y1, r1)
            line = len(buf)
            buf += colorline(0, st)
            buf[at + 1:at + 4] = (line - at).to_bytes(3, "big")
        elif kind == "sweep":  # (sweep, (cx, cy), start, end, stops); angles in half turns
            _, (cx, cy), a0, a1, st = p
            buf += struct.pack(">B", 8) + b"\0\0\0" + struct.pack(">hh", cx, cy) + f2dot14(a0) + f2dot14(a1)
            line = len(buf)
            buf += colorline(0, st)
            buf[at + 1:at + 4] = (line - at).to_bytes(3, "big")
        elif kind == "glyph":  # (glyph, gid, paint)
            buf += struct.pack(">B", 10) + b"\0\0\0" + struct.pack(">H", p[1])
            child = emit(p[2], buf)
            buf[at + 1:at + 4] = (child - at).to_bytes(3, "big")
        elif kind == "colrglyph":
            buf += struct.pack(">BH", 11, p[1])
        elif kind == "transform":  # (transform, (xx, yx, xy, yy, dx, dy), paint)
            buf += struct.pack(">B", 12) + b"\0" * 6
            affine = len(buf)
            buf += b"".join(fixed(v) for v in p[1])
            child = emit(p[2], buf)
            buf[at + 1:at + 4] = (child - at).to_bytes(3, "big")
            buf[at + 4:at + 7] = (affine - at).to_bytes(3, "big")
        elif kind == "translate":  # (translate, dx, dy, paint)
            buf += struct.pack(">B", 14) + b"\0\0\0" + struct.pack(">hh", p[1], p[2])
            child = emit(p[3], buf)
            buf[at + 1:at + 4] = (child - at).to_bytes(3, "big")
        elif kind == "rotate":  # (rotate, half turns, paint)
            buf += struct.pack(">B", 24) + b"\0\0\0" + f2dot14(p[1])
            child = emit(p[2], buf)
            buf[at + 1:at + 4] = (child - at).to_bytes(3, "big")
        elif kind == "composite":  # (composite, source, mode, backdrop)
            buf += struct.pack(">B", 32) + b"\0\0\0" + struct.pack(">B", p[2]) + b"\0\0\0"
            src = emit(p[1], buf)
            back = emit(p[3], buf)
            buf[at + 1:at + 4] = (src - at).to_bytes(3, "big")
            buf[at + 5:at + 8] = (back - at).to_bytes(3, "big")
        else:
            raise ValueError(kind)
        return at

    # v0 records.
    base0 = b""
    layer0 = b""
    n_layers = 0
    for gid in sorted(v0):
        base0 += struct.pack(">HHH", gid, n_layers, len(v0[gid]))
        for lg, idx in v0[gid]:
            layer0 += struct.pack(">HH", lg, idx)
            n_layers += 1
    # BaseGlyphList: header, records, paints.
    gids = sorted(v1)
    bgl = bytearray(struct.pack(">I", len(gids)) + b"\0" * (6 * len(gids)))
    for i, gid in enumerate(gids):
        at = emit(v1[gid], bgl)
        struct.pack_into(">HI", bgl, 4 + 6 * i, gid, at)
    # LayerList: header, offsets, paints (identical paints shared).
    ll = bytearray(struct.pack(">I", len(layers)) + b"\0" * (4 * len(layers)))
    seen = {}
    for i, p in enumerate(layers):
        key = id(p)
        if key not in seen:
            seen[key] = emit(p, ll)
        struct.pack_into(">I", ll, 4 + 4 * i, seen[key])
    # ClipList: one clip per glyph, each with its own box.
    clip_gids = sorted(clips)
    cl = bytearray(struct.pack(">BI", 1, len(clip_gids)) + b"\0" * (7 * len(clip_gids)))
    for i, gid in enumerate(clip_gids):
        box = len(cl)
        cl += struct.pack(">Bhhhh", 1, *clips[gid])
        struct.pack_into(">HH", cl, 5 + 7 * i, gid, gid)
        cl[5 + 7 * i + 4:5 + 7 * i + 7] = box.to_bytes(3, "big")
    header_len = 34
    off_base0 = header_len
    off_layer0 = off_base0 + len(base0)
    off_bgl = off_layer0 + len(layer0)
    off_ll = off_bgl + len(bgl)
    off_cl = off_ll + len(ll)
    off_vm = off_cl + (len(cl) if clips else 0)
    off_vs = off_vm + len(var_map)
    header = struct.pack(
        ">HHIIHIIIII", 1, len(v0), off_base0 if v0 else 0, off_layer0 if v0 else 0, n_layers,
        off_bgl, off_ll if layers else 0, off_cl if clips else 0, off_vm if var_map else 0,
        off_vs if var_store else 0,
    )
    return (header + base0 + layer0 + bytes(bgl) + bytes(ll) + (bytes(cl) if clips else b"")
            + var_map + var_store)


# --- Bitmaps ----------------------------------------------------------------

def png(w, h, pixel):
    """An RGBA PNG; `pixel(x, y)` gives (r, g, b, a), y down."""
    raw = b"".join(b"\0" + b"".join(bytes(pixel(x, y)) for x in range(w)) for y in range(h))

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


PX = 64  # bitmap size and strike ppem


def inside(x, y, cx, cy, r):
    return (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r


def smiley(x, y):
    if inside(x, y, 24, 24, 5) or inside(x, y, 40, 24, 5):
        return (40, 30, 20, 255)
    if 20 <= x < 44 and 40 <= y < 46:
        return (120, 40, 30, 255)
    if inside(x, y, 32, 32, 30):
        t = min(1.0, math.hypot(x - 26, y - 26) / 34)
        return (255, round(220 - 90 * t), round(60 - 40 * t), 255)
    return (0, 0, 0, 0)


def heart(x, y):
    if inside(x, y, 22, 24, 12) or inside(x, y, 42, 24, 12) or abs(x + 0.5 - 32) + abs(y + 0.5 - 30) <= 22 and y > 24:
        return (220, 30, 60, 255)
    return (0, 0, 0, 0)


def thumb(x, y):
    if 14 <= x < 40 and 28 <= y < 58 or inside(x, y, 38, 18, 10):
        return (250, 190, 70, 255)
    return (0, 0, 0, 0)


def flag(x, y):
    if 10 <= y < 54:
        return [(0, 50, 160, 255), (245, 245, 245, 255), (230, 30, 45, 255)][min(2, x * 3 // PX)]
    return (0, 0, 0, 0)


# --- Font files -------------------------------------------------------------

def sfnt(tables):
    tags = sorted(tables)
    n = len(tags)
    entry = 2 ** int(math.log2(n))
    out = struct.pack(">IHHHH", 0x00010000, n, entry * 16, int(math.log2(entry)), n * 16 - entry * 16)
    offset = 12 + 16 * n
    body = b""
    for tag in tags:
        data = tables[tag]
        checksum = sum(struct.unpack(">%dI" % ((len(data) + 3) // 4), data + b"\0" * (-len(data) % 4))) & 0xFFFFFFFF
        out += struct.pack(">4sIII", tag.encode(), checksum, offset + len(body), len(data))
        body += data + b"\0" * (-len(data) % 4)
    return out + body


def outline_font(family, glyphs, mapping, extra):
    gl = b""
    loca = []
    for g in glyphs:
        loca.append(len(gl))
        gl += glyf_data(g)
    loca.append(len(gl))
    bounds = [g.bounds() for g in glyphs if g.contours]
    total = (min(b[0] for b in bounds), min(b[1] for b in bounds), max(b[2] for b in bounds), max(b[3] for b in bounds))
    tables = {
        "head": head(total),
        "hhea": hhea(glyphs),
        "maxp": maxp(glyphs, True),
        "hmtx": hmtx(glyphs),
        "cmap": cmap(mapping),
        "name": name(family),
        "OS/2": os2(),
        "post": post(),
        "glyf": gl,
        "loca": b"".join(struct.pack(">I", o) for o in loca),
    }
    tables.update(extra)
    return sfnt(tables)


def bitmap_font(family, glyphs, mapping, extra):
    tables = {
        "head": head(BOX),
        "hhea": hhea(glyphs),
        "maxp": maxp(glyphs, False),
        "hmtx": hmtx(glyphs),
        "cmap": cmap(mapping),
        "name": name(family),
        "OS/2": os2(),
        "post": post(),
    }
    tables.update(extra)
    return sfnt(tables)


def colr_font():
    g = []

    def add(name, contours=(), advance=EMOJI_ADVANCE):
        g.append(Glyph(name, contours, advance))
        return len(g) - 1

    notdef = add(".notdef", [rect(50, 0, 550, 700), rect(100, 650, 500, 50)], 600)
    space = add("space", (), 300)
    zwj = add("zwj", (), 0)
    vs16 = add("vs16", (), 0)
    keycap_mark = add("keycap", (), 0)
    one = add("one", [rect(250, 0, 350, 700)], 600)
    ri_f = add("ri_f", [rect(100, 0, 500, 700)], 600)
    ri_r = add("ri_r", [rect(100, 0, 500, 700)], 600)
    grin = add("grin", [circle(550, 350, 450)])
    heart_g = add("heart", [circle(380, 470, 190), circle(720, 470, 190), diamond(550, 300, 340, 300)])
    thumbs = add("thumbs", [rect(250, -100, 650, 500), circle(600, 600, 180)])
    thumbs_mid = add("thumbs_medium", [rect(250, -100, 650, 500), circle(600, 600, 180)])
    tone = add("tone_medium", [rect(150, -50, 950, 750)])
    flag_fr = add("flag_fr", [rect(50, 0, 1050, 700)])
    key_one = add("keycap_one", [rect(100, -100, 1000, 800)])
    woman = add("woman", [circle(550, 550, 250), rect(250, -150, 850, 300)])
    laptop = add("laptop", [rect(200, 150, 900, 750), rect(100, -50, 1000, 150)])
    woman_laptop = add("woman_laptop", [rect(50, -150, 1050, 850)])
    # Layer glyphs.
    l_face = add("l_face", [circle(550, 350, 450)])
    l_eye_l = add("l_eye_l", [circle(400, 480, 60)])
    l_eye_r = add("l_eye_r", [circle(700, 480, 60)])
    l_mouth = add("l_mouth", [rect(330, 150, 770, 230)])
    l_heart_l = add("l_heart_l", [circle(380, 470, 190)])
    l_heart_r = add("l_heart_r", [circle(720, 470, 190)])
    l_heart_b = add("l_heart_b", [diamond(550, 300, 340, 300)])
    l_thumb = add("l_thumb", [rect(250, -100, 650, 500)])
    l_tip = add("l_tip", [circle(600, 600, 180)])
    l_tone = add("l_tone", [rect(150, -50, 950, 750)])
    l_stripe = [add("l_stripe%d" % i, [rect(50 + 333 * i, 0, 383 + 333 * i, 700)]) for i in range(3)]
    l_key = add("l_key", [rect(100, -100, 1000, 800)])
    l_bar = add("l_bar", [rect(500, 50, 620, 650)])
    l_head = add("l_head", [circle(550, 550, 250)])
    l_body = add("l_body", [rect(250, -150, 850, 300)])
    l_screen = add("l_screen", [rect(200, 150, 900, 750)])
    l_base = add("l_base", [rect(100, -50, 1000, 150)])
    l_shine = add("l_shine", [circle(450, 600, 120)])
    # Takes the text colour: a star in the foreground colour (palette
    # index 0xFFFF) with a dot in the palette's yellow.
    star = add("star", [diamond(550, 350, 450, 450)])
    l_star = add("l_star", [diamond(550, 350, 450, 450)])
    l_dot = add("l_dot", [circle(550, 350, 120)])
    # Composites in other modes: a blend, a source-in and a destination-out.
    art = add("art", [rect(50, -150, 1050, 850)])

    palette = [
        (255, 200, 40, 255),   # 0 yellow
        (240, 120, 20, 255),   # 1 orange
        (60, 40, 30, 255),     # 2 dark brown
        (220, 30, 60, 255),    # 3 red
        (250, 190, 70, 255),   # 4 light skin
        (160, 100, 60, 255),   # 5 medium skin
        (0, 50, 160, 255),     # 6 blue
        (245, 245, 245, 255),  # 7 white
        (150, 155, 165, 255),  # 8 grey
        (40, 110, 220, 255),   # 9 sky blue
        (90, 30, 140, 255),    # 10 purple
        (40, 160, 90, 255),    # 11 green
    ]
    v0 = {
        heart_g: [(l_heart_l, 3), (l_heart_r, 3), (l_heart_b, 3)],
        tone: [(l_tone, 5)],
        flag_fr: [(l_stripe[0], 6), (l_stripe[1], 7), (l_stripe[2], 3)],
        star: [(l_star, 0xFFFF), (l_dot, 0)],
    }
    layers = []

    def colr_layers(*paints):
        first = len(layers)
        layers.extend(paints)
        return ("layers", first, len(paints))

    face = ("radial", (450, 450, 0), (550, 350, 450), stops((0, 0, 1), (1, 1, 1)))
    v1 = {
        grin: colr_layers(
            ("glyph", l_face, face),
            ("glyph", l_eye_l, solid(2)),
            ("glyph", l_eye_r, solid(2)),
            ("glyph", l_mouth, solid(2, 0.8)),
        ),
        thumbs: colr_layers(("glyph", l_thumb, solid(4)), ("glyph", l_tip, solid(4))),
        thumbs_mid: colr_layers(("glyph", l_thumb, solid(5)), ("glyph", l_tip, solid(5))),
        key_one: colr_layers(
            ("glyph", l_key, ("linear", (100, -100), (1000, 800), (1000, -100), stops((0, 8, 1), (1, 9, 1)))),
            ("glyph", l_bar, solid(7)),
        ),
        woman: colr_layers(
            ("glyph", l_head, solid(4)),
            ("glyph", l_body, ("sweep", (550, 75), 0, 1.99, stops((0, 10, 1), (1, 11, 1)))),
        ),
        laptop: colr_layers(
            ("composite", ("glyph", l_shine, solid(7, 0.6)), 3, ("glyph", l_screen, solid(8))),
            ("rotate", 0.02, ("glyph", l_base, solid(2))),
        ),
        art: colr_layers(
            ("composite", ("glyph", l_dot, solid(3)), 23, ("glyph", l_face, solid(0))),
            ("composite", ("glyph", l_eye_l, solid(6)), 5, ("glyph", l_heart_l, solid(11))),
            ("composite", ("glyph", l_eye_r, solid(7)), 8, ("glyph", l_tip, solid(1))),
        ),
        woman_laptop: colr_layers(
            ("transform", (0.6, 0, 0, 0.6, 0, 300), ("colrglyph", woman)),
            ("translate", 420, 0, ("transform", (0.55, 0, 0.1, 0.55, 0, 0), ("colrglyph", laptop))),
        ),
    }
    clips = {grin: BOX, woman_laptop: BOX}
    mapping = {
        0x20: space, 0x31: one, 0x200D: zwj, 0x20E3: keycap_mark, 0xFE0F: vs16,
        0x2764: heart_g, 0x1F1EB: ri_f, 0x1F1F7: ri_r, 0x1F3FD: tone, 0x1F44D: thumbs,
        0x1F469: woman, 0x1F4BB: laptop, 0x1F600: grin, 0x2B50: star, 0x1F3A8: art,
    }
    ligatures = [
        ([ri_f, ri_r], flag_fr),
        ([one, vs16, keycap_mark], key_one),
        ([one, keycap_mark], key_one),
        ([thumbs, tone], thumbs_mid),
        ([woman, zwj, laptop], woman_laptop),
    ]
    extra = {"GSUB": gsub(ligatures), "COLR": colrv1(v0, v1, clips, layers), "CPAL": cpal(palette)}
    assert notdef == 0
    return outline_font("Emoji Test COLR", g, mapping, extra)


def bitmap_glyphs():
    """Glyphs and cmap shared by the bitmap fonts: the colour glyphs first
    (1..4), so one CBLC index subtable covers them."""
    names = [".notdef", "grin", "heart", "thumbs", "flag_fr", "space", "zwj", "vs16", "ri_f", "ri_r"]
    advances = {"space": 300, "zwj": 0, "vs16": 0, ".notdef": 600, "ri_f": 600, "ri_r": 600}
    glyphs = [Glyph(n, (), advances.get(n, EMOJI_ADVANCE)) for n in names]
    gid = {n: i for i, n in enumerate(names)}
    mapping = {0x20: gid["space"], 0x200D: gid["zwj"], 0xFE0F: gid["vs16"], 0x2764: gid["heart"],
               0x1F1EB: gid["ri_f"], 0x1F1F7: gid["ri_r"], 0x1F44D: gid["thumbs"], 0x1F600: gid["grin"]}
    images = [png(PX, PX, f) for f in (smiley, heart, thumb, flag)]
    return glyphs, gid, mapping, images


# Placement of a 64 px bitmap at ppem 64: from x 50 / 1000 em, top at 850.
BEARING_X = round(BOX[0] * PX / UPEM)
BEARING_Y = round(BOX[3] * PX / UPEM)
ADVANCE_PX = round(EMOJI_ADVANCE * PX / UPEM)


def cbdt_font():
    glyphs, gid, mapping, images = bitmap_glyphs()
    cbdt = struct.pack(">HH", 3, 0)
    offsets = []
    for img in images:
        offsets.append(len(cbdt))
        cbdt += struct.pack(">BBbbBI", PX, PX, BEARING_X, BEARING_Y, ADVANCE_PX, len(img)) + img
    first, last = 1, len(images)
    image_data = offsets[0]
    # IndexSubTable format 1, image format 17 (small metrics, PNG).
    sub = struct.pack(">HHI", 1, 17, image_data)
    sub += b"".join(struct.pack(">I", o - image_data) for o in offsets + [len(cbdt)])
    array = struct.pack(">HHI", first, last, 8) + sub
    line = struct.pack(">bbBbbbbbbbbb", BEARING_Y, BEARING_Y - PX, PX, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    size = struct.pack(">IIII", 8 + 48, len(array), 1, 0) + line + line
    size += struct.pack(">HHBBBb", first, last, PX, PX, 32, 1)
    cblc = struct.pack(">HHI", 3, 0, 1) + size + array
    gsub_table = gsub([([gid["ri_f"], gid["ri_r"]], gid["flag_fr"])])
    return bitmap_font("Emoji Test CBDT", glyphs, mapping, {"CBDT": cbdt, "CBLC": cblc, "GSUB": gsub_table})


def sbix_strike(n, ppem, images):
    """An sbix strike of `n` glyphs at `ppem`, with `images` for glyphs 1.."""
    strike = bytearray(struct.pack(">HH", ppem, 72) + b"\0" * (4 * (n + 1)))
    for i in range(n):
        struct.pack_into(">I", strike, 4 + 4 * i, len(strike))
        if 1 <= i <= len(images):
            x, y = BEARING_X * ppem // PX, (BEARING_Y - PX) * ppem // PX
            strike += struct.pack(">hh4s", x, y, b"png ") + images[i - 1]
    struct.pack_into(">I", strike, 4 + 4 * n, len(strike))
    return bytes(strike)


def sbix_font():
    """Two strikes: the 64 ppem one, which the engine picks as the largest,
    after one at half the size."""
    glyphs, gid, mapping, images = bitmap_glyphs()
    n = len(glyphs)
    half = PX // 2
    small = [png(half, half, lambda x, y, f=f: f(2 * x, 2 * y)) for f in (smiley, heart, thumb, flag)]
    strikes = [sbix_strike(n, half, small), sbix_strike(n, PX, images)]
    header = 8 + 4 * len(strikes)
    sbix = struct.pack(">HHI", 1, 1, len(strikes))
    sbix += struct.pack(">I", header) + struct.pack(">I", header + len(strikes[0]))
    sbix += b"".join(strikes)
    gsub_table = gsub([([gid["ri_f"], gid["ri_r"]], gid["flag_fr"])])
    return bitmap_font("Emoji Test sbix", glyphs, mapping, {"sbix": sbix, "GSUB": gsub_table})


def svg_table(docs):
    """An SVG table of `docs`, (first glyph, last glyph, document bytes)."""
    records = struct.pack(">H", len(docs))
    body = b""
    first = 2 + 12 * len(docs)
    for start, end, data in docs:
        records += struct.pack(">HHII", start, end, first + len(body), len(data))
        body += data
    return struct.pack(">HII", 0, 10, 0) + records + body


def svg_font():
    names = [".notdef", "space", "grin", "heart", "star", "thumbs", "flag"]
    shapes = {
        ".notdef": [rect(50, 0, 550, 700)],
        "grin": [circle(550, 350, 450)],
        "heart": [circle(380, 470, 190), circle(720, 470, 190), diamond(550, 300, 340, 300)],
        "star": [diamond(550, 350, 450, 450)],
        "thumbs": [rect(250, -100, 650, 500)],
        "flag": [rect(50, 0, 1050, 700)],
    }
    advances = {".notdef": 600, "space": 300}
    glyphs = [Glyph(n, shapes.get(n, ()), advances.get(n, EMOJI_ADVANCE)) for n in names]
    gid = {n: i for i, n in enumerate(names)}
    ns = 'xmlns="http://www.w3.org/2000/svg"'
    # Drawn at half size in a viewBox half the em, so it fills x 100..1000
    # and y -800..100 (y down from the baseline); no glyph id, as the only
    # glyph of its document; compressed.
    grin = (
        f'<?xml version="1.0"?><svg {ns} viewBox="0 0 500 500">'
        '<defs><radialGradient id="face" cx="275" cy="-175" r="225" gradientUnits="userSpaceOnUse">'
        '<stop offset="0" stop-color="#ffc828"/><stop offset="1" stop-color="#f07814"/></radialGradient></defs>'
        '<circle cx="275" cy="-175" r="225" fill="url(#face)"/>'
        '<circle cx="200" cy="-240" r="30" fill="#3c281e"/><circle cx="350" cy="-240" r="30" fill="#3c281e"/>'
        "</svg>"
    )
    # Two glyphs and the gradient they share; the star in the text colour.
    pair = (
        f'<svg {ns}><defs><linearGradient id="red" x1="0" y1="0" x2="0" y2="1">'
        '<stop offset="0" stop-color="#dc1e3c"/><stop offset="1" stop-color="#a01428"/></linearGradient></defs>'
        f'<g id="glyph{gid["heart"]}"><path fill="url(#red)" d="M210 -470A170 170 0 0 1 550 -470'
        'A170 170 0 0 1 890 -470L550 0Z"/></g>'
        f'<g id="glyph{gid["star"]}"><path fill="context-fill" d="M550 -800L1000 -350L550 100L100 -350Z"/>'
        '<circle cx="550" cy="-350" r="120" fill="#ffc828"/></g>'
        "</svg>"
    )
    # Glyphs in a group: drawn by reference.
    nested = (
        f'<svg {ns}><g>'
        f'<g id="glyph{gid["thumbs"]}"><rect x="250" y="-500" width="400" height="600" fill="#fabe46"/></g>'
        f'<g id="glyph{gid["flag"]}"><rect x="50" y="-700" width="1000" height="700" fill="#0032a0"/></g>'
        "</g></svg>"
    )
    docs = [
        (gid["grin"], gid["grin"], gzip.compress(grin.encode(), mtime=0)),
        (gid["heart"], gid["star"], pair.encode()),
        (gid["thumbs"], gid["flag"], nested.encode()),
    ]
    mapping = {0x20: gid["space"], 0x1F600: gid["grin"], 0x2764: gid["heart"], 0x2B50: gid["star"],
               0x1F44D: gid["thumbs"], 0x1F3C1: gid["flag"]}
    return outline_font("Emoji Test SVG", glyphs, mapping, {"SVG ": svg_table(docs)})


def fanout_font(empty=False):
    levels = 8
    width = 255
    g = [Glyph(".notdef", [rect(50, 0, 550, 700)], 600), Glyph("bomb", [circle(550, 350, 450)])]
    level_gids = []
    for i in range(levels):
        g.append(Glyph("level%d" % i, [rect(100, 0, 1000, 800)]))
        level_gids.append(len(g) - 1)
    g.append(Glyph("leaf", [rect(400, 300, 700, 600)]))
    leaf = len(g) - 1
    layers = []
    v1 = {}
    chain = [1] + level_gids
    for depth, gid in enumerate(chain):
        if depth + 1 < len(chain):
            child = ("colrglyph", chain[depth + 1])
            first = len(layers)
            layers.extend([child] * width)
            v1[gid] = ("layers", first, width)
        elif empty:
            v1[gid] = ("layers", len(layers), 0)
        else:
            v1[gid] = ("glyph", leaf, solid(0))
    extra = {"COLR": colrv1({}, v1, {}, layers), "CPAL": cpal([(200, 30, 30, 255)])}
    return outline_font("COLR Fanout", g, {0x1F600: 1}, extra)


def main():
    outputs = {
        FONTS / "EmojiTest-COLR.ttf": colr_font(),
        FONTS / "EmojiTest-CBDT.ttf": cbdt_font(),
        FONTS / "EmojiTest-sbix.ttf": sbix_font(),
        FONTS / "EmojiTest-SVG.ttf": svg_font(),
        FONTS / "limits" / "colr-fanout.ttf": fanout_font(),
        FONTS / "limits" / "colr-fanout-empty.ttf": fanout_font(empty=True),
    }
    for path, data in outputs.items():
        path.write_bytes(data)
        print(f"{path.relative_to(ROOT)}: {len(data)} bytes")


if __name__ == "__main__":
    main()
