#!/usr/bin/env python3
"""Generates the variable test fonts, written from scratch (no font tools,
no third-party artwork), so they are small and free of licences:

- fixtures/fonts/VarTest-glyf.ttf: TrueType outlines for " EFHILOT" made of
  rectangles, with a `wght` axis (100 to 900, default 400; the stems grow)
  and a `wdth` axis (50 to 100, default 100; the letters narrow), `gvar`
  deltas for the outlines and their advances (phantom points), `HVAR` for
  the advances too, `MVAR` for the x-height and cap height, an `avar` 1.0
  map that bends `wght`, and three named instances;
- fixtures/fonts/VarTest-CFF2.otf: the same letters as CFF2 outlines with
  `blend`, the same axes, `HVAR` and `avar`;
- fixtures/fonts/VarTest-layout.ttf: VarTest-glyf with layout tables that
  vary: `GSUB` with `rvrn`, whose FeatureVariations turn I into L from
  wght 900, and `GPOS` with `kern`, T before O, whose kerning grows with
  wght through a VariationIndex device into `GDEF`'s variation store;
- fixtures/fonts/VarTest-COLRv1.ttf: a square (U+25A0) in COLR v1 whose
  colour fades with wght (PaintVarSolid), and outlines that do not vary;
- fixtures/fonts/VarTest-opsz.ttf: VarTest-glyf with an `opsz` axis (6 to
  72, default 12; the stems grow) in place of `wght`;
- fixtures/fonts/limits/var-avar2.ttf: VarTest-glyf with an `avar` 2.0
  table, which the engine refuses;
- fixtures/fonts/limits/var-64-axes.ttf: VarTest-glyf with 64 axes, which
  the engine refuses;
- fixtures/fonts/limits/var-no-gvar.ttf: VarTest-glyf without `gvar`, and
  fixtures/fonts/limits/var-gvar-cut.ttf with its `gvar` cut short, which
  the engine refuses;
- fixtures/fonts/limits/var-cff-and-cff2.otf: VarTest-CFF2 with a `CFF `
  table too, which the engine refuses: CFF, read first, has no variations;
- fixtures/docs/variable-fonts.json: both fonts at several instances, text
  at the optical size of its size, and SVG text matched to the nearest
  declared instance, or drawn at its weight and width within a
  declaration's ranges.

Every coordinate is a sum of a term in the width and a term in the stem,
so the deltas of the single-axis regions add up to the exact outline at
any location.

The output is committed; scripts/check-generated.sh regenerates it and
fails when it differs, so edit this script, not the fonts.

  scripts/fixtures/variable_fonts.py
"""
import pathlib
import struct
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import common  # noqa: E402
import emoji_fonts as ef  # noqa: E402

ROOT = ef.ROOT
FONTS = ef.FONTS

# wght: user 100..900, default 400; the stem in font units at each end.
STEM = {-1: 40, 0: 90, 1: 190}
# wdth: user 50..100, default 100; the horizontal scale at each end.
SCALE = {-1: 0.6, 0: 1.0}
# avar 1.0 for wght: user 650 (normalised 0.5) maps to 0.4.
AVAR_WGHT = [(-1.0, -1.0), (0.0, 0.0), (0.5, 0.4), (1.0, 1.0)]
X_HEIGHT, CAP_HEIGHT = 500, 700
# MVAR deltas at wght 900.
X_HEIGHT_BOLD, CAP_HEIGHT_BOLD = 20, 10


def letters(s, f):
    """Each glyph's rectangles and advance, for stem `s` and scale `f`:
    every coordinate is linear in `s` plus linear in `f`."""
    c = CAP_HEIGHT
    bar = s * 0.8

    def r(x0, y0, x1, y1):
        return (round(x0), round(y0), round(x1), round(y1))

    w = 600 * f
    return {
        "E": ([r(60 * f, 0, 60 * f + s, c), r(60 * f + s, c - bar, 500 * f + s, c),
               r(60 * f + s, c / 2 - bar / 2, 440 * f + s, c / 2 + bar / 2),
               r(60 * f + s, 0, 500 * f + s, bar)], w - 40 * f + s),
        "F": ([r(60 * f, 0, 60 * f + s, c), r(60 * f + s, c - bar, 480 * f + s, c),
               r(60 * f + s, c / 2 - bar / 2, 420 * f + s, c / 2 + bar / 2)], w - 60 * f + s),
        "H": ([r(60 * f, 0, 60 * f + s, c), r(480 * f + s, 0, 480 * f + 2 * s, c),
               r(60 * f + s, c / 2 - bar / 2, 480 * f + s, c / 2 + bar / 2)], 540 * f + 2 * s),
        "I": ([r(60 * f, 0, 60 * f + s, c)], 180 * f + s),
        "L": ([r(60 * f, 0, 60 * f + s, c), r(60 * f + s, 0, 460 * f + s, bar)], 500 * f + s),
        "O": ([r(60 * f, 0, 60 * f + s, c), r(500 * f + s, 0, 500 * f + 2 * s, c),
               r(60 * f + s, 0, 500 * f + s, bar), r(60 * f + s, c - bar, 500 * f + s, c)], 560 * f + 2 * s),
        "T": ([r(280 * f, 0, 280 * f + s, c - bar), r(40 * f, c - bar, 520 * f + s, c)], 560 * f + s),
    }


NAMES = ["E", "F", "H", "I", "L", "O", "T"]


def glyphs_at(stem, scale):
    """The glyphs (.notdef, space, then NAMES) at one location."""
    out = [ef.Glyph(".notdef", [ef.rect(50, 0, 450, 700), list(reversed(ef.rect(100, 50, 400, 650)))], 500),
           ef.Glyph("space", [], 250)]
    shapes = letters(stem, scale)
    for n in NAMES:
        rects, adv = shapes[n]
        out.append(ef.Glyph(n, [ef.rect(*r) for r in rects], round(adv)))
    return out


DEFAULT = glyphs_at(STEM[0], SCALE[0])
# One entry per region, in this order: wght at -1, wght at +1, wdth at -1.
REGIONS = [((-1, -1, 0), (0, 0, 0)), ((0, 1, 1), (0, 0, 0)), ((0, 0, 0), (-1, -1, 0))]
EXTREMES = [glyphs_at(STEM[-1], SCALE[0]), glyphs_at(STEM[1], SCALE[0]), glyphs_at(STEM[0], SCALE[-1])]
MAPPING = {0x20: 1, **{ord(n): i + 2 for i, n in enumerate(NAMES)}}
AXES = [(b"wght", 100, 400, 900, 256), (b"wdth", 50, 100, 100, 257)]
# The first axis optical size instead: the same outlines at its ends.
OPSZ_AXES = [(b"opsz", 6, 12, 72, 256), (b"wdth", 50, 100, 100, 257)]


def f2dot14(v):
    return struct.pack(">h", round(v * 16384))


def fixed(v):
    return struct.pack(">i", round(v * 65536))


def points(g):
    return [(x, y) for c in g.contours for x, y, _ in c]


def deltas(gid):
    """Per region, the (dx, dy) of each point of glyph `gid`, then of its
    four phantom points (the advance moves the second)."""
    base = DEFAULT[gid]
    out = []
    for ext in EXTREMES:
        g = ext[gid]
        d = [(x1 - x0, y1 - y0) for (x0, y0), (x1, y1) in zip(points(base), points(g))]
        d += [(0, 0), (g.advance - base.advance, 0), (0, 0), (0, 0)]
        out.append(d)
    return out


def packed_words(values):
    out = b""
    for i in range(0, len(values), 64):
        run = values[i:i + 64]
        out += bytes([0x40 | (len(run) - 1)]) + b"".join(struct.pack(">h", v) for v in run)
    return out


def gvar(axis_count=2, extra_axes=0):
    data = []
    for gid in range(len(DEFAULT)):
        if not DEFAULT[gid].contours:
            data.append(b"")
            continue
        tuples = deltas(gid)
        headers = b""
        bodies = b""
        for (peaks, d) in zip(REGIONS, tuples):
            body = packed_words([x for x, _ in d]) + packed_words([y for _, y in d])
            peak = b"".join(f2dot14(p[1]) for p in peaks) + b"\0\0" * extra_axes
            headers += struct.pack(">HH", len(body), 0x8000) + peak
            bodies += body
        # Shared point numbers: one byte 0, every point.
        shared = b"\0"
        offset = 4 + len(headers)
        blob = struct.pack(">HH", 0x8000 | len(tuples), offset) + headers + shared + bodies
        data.append(blob + b"\0" * (len(blob) % 2))
    offsets = [0]
    for d in data:
        offsets.append(offsets[-1] + len(d))
    head_len = 20 + 4 * len(offsets)
    return (struct.pack(">HHHHIHHI", 1, 0, axis_count + extra_axes, 0, head_len, len(DEFAULT), 1, head_len)
            + b"".join(struct.pack(">I", o) for o in offsets) + b"".join(data))


def item_variation_store(rows, extra_axes=0):
    """Format 1 with the three REGIONS and one data subtable of `rows`
    (one int16 delta per region each)."""
    axis_count = 2 + extra_axes
    regions = struct.pack(">HH", axis_count, len(REGIONS))
    for per_axis in REGIONS:
        for start, peak, end in per_axis:
            regions += f2dot14(start) + f2dot14(peak) + f2dot14(end)
        regions += (f2dot14(0) * 3) * extra_axes
    sub = struct.pack(">HHH", len(rows), len(REGIONS), len(REGIONS))
    sub += b"".join(struct.pack(">H", i) for i in range(len(REGIONS)))
    sub += b"".join(b"".join(struct.pack(">h", v) for v in row) for row in rows)
    regions_off = 8 + 4
    sub_off = regions_off + len(regions)
    return struct.pack(">HIHI", 1, regions_off, 1, sub_off) + regions + sub


def hvar(extra_axes=0):
    rows = [[ext[g].advance - DEFAULT[g].advance for ext in EXTREMES] for g in range(len(DEFAULT))]
    store = item_variation_store(rows, extra_axes)
    # No mapping: glyph id is the inner index of outer index 0.
    return struct.pack(">HHIIII", 1, 0, 20, 0, 0, 0) + store


def mvar(extra_axes=0):
    rows = [[0, X_HEIGHT_BOLD, 0], [0, CAP_HEIGHT_BOLD, 0]]
    store = item_variation_store(rows, extra_axes)
    records = [(b"cpht", 0, 1), (b"xhgt", 0, 0)]
    header = struct.pack(">HHHHHH", 1, 0, 0, 8, len(records), 12 + 8 * len(records))
    return header + b"".join(struct.pack(">4sHH", t, o, i) for t, o, i in records) + store


def avar(version=1, extra_axes=0):
    maps = [AVAR_WGHT, [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)]] + [[(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)]] * extra_axes
    out = struct.pack(">HHHH", version, 0, 0, len(maps))
    for m in maps:
        out += struct.pack(">H", len(m)) + b"".join(f2dot14(a) + f2dot14(b) for a, b in m)
    if version == 2:
        # No axis index map and no variation store: offsets 0.
        out += struct.pack(">II", 0, 0)
    return out


def fvar(extra_axes=0, axes=AXES):
    instances = [(2, (axes[0][2], 100)), (258, (700, 100)), (259, (400, 50))]
    if axes is not AXES:
        instances = instances[:1]
    axes = list(axes) + [(b"x%03d" % i, 0, 0, 1, 256) for i in range(extra_axes)]
    n = len(axes)
    out = struct.pack(">HHHHHHHH", 1, 0, 16, 2, n, 20, len(instances), 4 + 4 * n)
    for tag, lo, default, hi, name_id in axes:
        out += tag + fixed(lo) + fixed(default) + fixed(hi) + struct.pack(">HH", 0, name_id)
    for name_id, coords in instances:
        out += struct.pack(">HH", name_id, 0) + b"".join(fixed(v) for v in coords) + fixed(0) * extra_axes
    return out


def name(family="VarTest", first_axis="Weight"):
    records = [(1, family), (2, "Regular"), (4, f"{family} Regular"), (6, f"{family}-Regular"),
               (25, family), (256, first_axis), (257, "Width"), (258, "Bold"), (259, "Condensed")]
    data = b""
    entries = b""
    for nid, text in records:
        s = text.encode("utf-16-be")
        entries += struct.pack(">HHHHHH", 3, 1, 0x409, nid, len(s), len(data))
        data += s
    return struct.pack(">HHH", 0, len(records), 6 + 12 * len(records)) + entries + data


# --- Layout tables that vary ------------------------------------------------

GID = {n: i + 2 for i, n in enumerate(NAMES)}
# T before O kerns by KERN at the default, and by KERN + KERN_BOLD at wght 900.
KERN, KERN_BOLD = -50, -100


def coverage(gids):
    return struct.pack(">HH", 1, len(gids)) + b"".join(struct.pack(">H", g) for g in gids)


def layout_lists(feature, lookup):
    """A ScriptList with DFLT, a FeatureList with `feature` (a tag and the
    indices of its lookups) and a LookupList with `lookup`, each to be
    placed at an offset of its own."""
    tag, lookups = feature
    langsys = struct.pack(">HHHH", 0, 0xFFFF, 1, 0)
    script = struct.pack(">HH", 4, 0) + langsys
    script_list = struct.pack(">H4sH", 1, b"DFLT", 8) + script
    feature_table = struct.pack(">HH", 0, len(lookups)) + b"".join(struct.pack(">H", i) for i in lookups)
    feature_list = struct.pack(">H4sH", 1, tag, 8) + feature_table
    lookup_list = struct.pack(">HH", 1, 4) + lookup
    return script_list, feature_list, lookup_list


def lookup(kind, subtable):
    return struct.pack(">HHHH", kind, 0, 1, 8) + subtable


def gsub():
    """GSUB 1.1 with `rvrn` and no lookups, and a FeatureVariations record
    that, from wght 0.8 (normalised), swaps in a feature with a lookup
    turning I into L."""
    single = struct.pack(">HHHH", 2, 8, 1, GID["L"]) + coverage([GID["I"]])
    script_list, feature_list, lookup_list = layout_lists((b"rvrn", []), lookup(1, single))
    condition = struct.pack(">HH", 1, 0) + f2dot14(0.8) + f2dot14(1.0)
    condition_set = struct.pack(">HI", 1, 6) + condition
    alternate = struct.pack(">HHH", 0, 1, 0)
    substitution = struct.pack(">HHHHI", 1, 0, 1, 0, 12) + alternate
    variations_body_off = 8 + 8
    variations = (struct.pack(">HHI", 1, 0, 1)
                  + struct.pack(">II", variations_body_off, variations_body_off + len(condition_set))
                  + condition_set + substitution)
    header_len = 14
    script_off = header_len
    feature_off = script_off + len(script_list)
    lookup_off = feature_off + len(feature_list)
    variations_off = lookup_off + len(lookup_list)
    return (struct.pack(">HHHHHI", 1, 1, script_off, feature_off, lookup_off, variations_off)
            + script_list + feature_list + lookup_list + variations)


def gpos():
    """GPOS with `kern`: T before O, its advance adjusted by KERN and a
    VariationIndex device into GDEF's store. A pair of classes (PairPos
    format 2): ttf-parser 0.25 finds no device table from the glyph pairs
    of format 1, whose offsets it reads within the pair records."""
    # Class 1 of the first glyphs is T, of the second O; class 0 the rest.
    # Each record: XAdvance and XAdvDevice for the first glyph.
    header_len, records_len = 16, 4 * 4
    device_off = header_len + records_len
    coverage_off = device_off + 6
    class1_off = coverage_off + 6
    class2_off = class1_off + 8
    records = struct.pack(">hHhH", 0, 0, 0, 0) + struct.pack(">hHhH", 0, 0, KERN, device_off)
    pair_pos = (struct.pack(">HHHHHHHH", 2, coverage_off, 0x0044, 0, class1_off, class2_off, 2, 2)
                + records + struct.pack(">HHH", 0, 0, 0x8000) + coverage([GID["T"]])
                + struct.pack(">HHHH", 1, GID["T"], 1, 1) + struct.pack(">HHHH", 1, GID["O"], 1, 1))
    script_list, feature_list, lookup_list = layout_lists((b"kern", [0]), lookup(2, pair_pos))
    header_len = 10
    script_off = header_len
    feature_off = script_off + len(script_list)
    lookup_off = feature_off + len(feature_list)
    return struct.pack(">HHHHH", 1, 0, script_off, feature_off, lookup_off) + script_list + feature_list + lookup_list


def gdef(extra_axes=0):
    """GDEF 1.3 with nothing but the item variation store of the kerning."""
    store = item_variation_store([[0, KERN_BOLD, 0]], extra_axes)
    return struct.pack(">HHHHHHHI", 1, 3, 0, 0, 0, 0, 0, 18) + store


def glyf_font(avar_version=1, extra_axes=0, gvar_len=None, layout=False, opsz=False):
    variations = gvar(extra_axes=extra_axes)
    extra = {
        "fvar": fvar(extra_axes, OPSZ_AXES if opsz else AXES),
        "gvar": variations[:gvar_len],
        "HVAR": hvar(extra_axes),
        "MVAR": mvar(extra_axes),
        "avar": avar(avar_version, extra_axes),
        "name": name("VarTestOpsz", "Optical size") if opsz else name(),
    }
    if layout:
        extra.update({"GSUB": gsub(), "GPOS": gpos(), "GDEF": gdef(extra_axes)})
    if gvar_len == 0:
        del extra["gvar"]
    return ef.outline_font("VarTest", DEFAULT, MAPPING, extra)


def colr_font():
    """A square whose COLR v1 paint (PaintVarSolid) fades from opaque at
    the default to half at wght 900, with an empty `gvar`: the outlines do
    not vary."""
    glyphs = [ef.Glyph(".notdef", [ef.rect(50, 0, 450, 700)], 500), ef.Glyph("square", [ef.rect(100, 0, 900, 800)], 1000)]
    # Alpha deltas in F2Dot14, per region: wght at -1, wght at +1, wdth at -1.
    store = item_variation_store([[0, -8192, 0]])
    v1 = {1: ("glyph", 1, ("varsolid", 0, 1.0, 0))}
    gvar_empty = (struct.pack(">HHHHIHHI", 1, 0, 2, 0, 20, len(glyphs), 0, 20 + 2 * (len(glyphs) + 1))
                  + b"\0" * (2 * (len(glyphs) + 1)))
    extra = {
        "fvar": fvar(),
        "gvar": gvar_empty,
        # DeltaSetIndexMap format 0, one-byte entries: variation index 0 is
        # outer 0, inner 0.
        "COLR": ef.colrv1({}, v1, {}, var_map=struct.pack(">BBHB", 0, 0, 1, 0), var_store=store),
        "CPAL": ef.cpal([(30, 120, 200, 255)]),
        "name": name(),
    }
    return ef.outline_font("VarTest", glyphs, {0x25A0: 1}, extra)


# --- CFF2 -------------------------------------------------------------------

def cff2_int(v):
    """A DICT or charstring integer operand (the encodings CFF2 shares)."""
    if -107 <= v <= 107:
        return bytes([v + 139])
    if 108 <= v <= 1131:
        v -= 108
        return bytes([(v >> 8) + 247, v & 0xFF])
    if -1131 <= v <= -108:
        v = -v - 108
        return bytes([(v >> 8) + 251, v & 0xFF])
    return bytes([28]) + struct.pack(">h", v)


def dict_int32(v):
    return bytes([29]) + struct.pack(">i", v)


def index(items):
    """A CFF2 INDEX (32-bit count, 4-byte offsets)."""
    if not items:
        return struct.pack(">I", 0)
    offsets = [1]
    for it in items:
        offsets.append(offsets[-1] + len(it))
    return (struct.pack(">IB", len(items), 4) + b"".join(struct.pack(">I", o) for o in offsets)
            + b"".join(items))


def charstring(gid):
    """The outline of glyph `gid` with `blend`: each move and line is
    relative, and every delta in it has one value per region."""
    base = DEFAULT[gid]
    if not base.contours:
        return b""
    ext = [e[gid] for e in EXTREMES]
    out = b""
    prev = [(0, 0)] + [(0, 0)] * len(ext)

    def operand_pairs(cur):
        """dx dy of the default, then per region their differences, blended."""
        nonlocal prev
        dx = [c[0] - p[0] for c, p in zip(cur, prev)]
        dy = [c[1] - p[1] for c, p in zip(cur, prev)]
        prev = cur
        vals = [dx[0], dy[0]]
        deltas = []
        for i in range(1, len(cur)):
            deltas += [dx[i] - dx[0]]
        deltas_y = [dy[i] - dy[0] for i in range(1, len(cur))]
        # blend: n defaults, then n * regions deltas, then n, then the operator.
        return b"".join(cff2_int(v) for v in vals) + b"".join(
            cff2_int(v) for v in deltas + deltas_y) + cff2_int(2) + bytes([16])

    for ci, contour in enumerate(base.contours):
        pts = [[(c[k][0], c[k][1]) for c in [contour] + [e.contours[ci] for e in ext]] for k in range(len(contour))]
        for k, pt in enumerate(pts):
            out += operand_pairs(pt) + bytes([21 if k == 0 else 5])  # rmoveto, rlineto
    return out


def cff2_table():
    charstrings = index([charstring(g) for g in range(len(DEFAULT))])
    # Variation store: the three regions, one data subtable with no rows.
    vstore_body = item_variation_store([])
    vstore = struct.pack(">H", len(vstore_body)) + vstore_body
    private = b""  # empty Private DICT: defaults
    fd = dict_int32(len(private)) + dict_int32(0) + bytes([18])  # Private size offset, patched below
    fdarray = index([fd])
    # Top DICT: CharStrings (17), FontDICTArray (12 36), vstore (24), FontMatrix default.
    def top(cs_off, fd_off, vs_off):
        return dict_int32(cs_off) + bytes([17]) + dict_int32(fd_off) + bytes([12, 36]) + dict_int32(vs_off) + bytes([24])
    top_len = len(top(0, 0, 0))
    header = struct.pack(">BBBH", 2, 0, 5, top_len)
    global_subrs = index([])
    start = len(header) + top_len + len(global_subrs)
    vs_off = start
    cs_off = vs_off + len(vstore)
    fd_off = cs_off + len(charstrings)
    private_off = fd_off + len(fdarray)
    fd = dict_int32(len(private)) + dict_int32(private_off) + bytes([18])
    fdarray = index([fd])
    return header + top(cs_off, fd_off, vs_off) + global_subrs + vstore + charstrings + fdarray + private


def cff2_font(cff=False):
    bounds = [g.bounds() for g in DEFAULT if g.contours]
    total = (min(b[0] for b in bounds), min(b[1] for b in bounds), max(b[2] for b in bounds), max(b[3] for b in bounds))
    tables = {
        "head": ef.head(total, loca_long=False),
        "hhea": ef.hhea(DEFAULT),
        "maxp": struct.pack(">IH", 0x00005000, len(DEFAULT)),
        "hmtx": ef.hmtx(DEFAULT),
        "cmap": ef.cmap(MAPPING),
        "name": name(),
        "OS/2": ef.os2(),
        "post": ef.post(),
        "CFF2": cff2_table(),
        "fvar": fvar(),
        "HVAR": hvar(),
        "avar": avar(),
    }
    if cff:
        # A CFF header and nothing else.
        tables["CFF "] = bytes([1, 0, 4, 4])
    out = ef.sfnt(tables)
    # OpenType with CFF outlines: the sfnt version is 'OTTO'.
    return b"OTTO" + out[4:]


def document():
    """Each instance on a line, labelled in Inter."""
    instances = [
        ("VarTest-100", "VarTest-glyf", 100, {"wght": 100}),
        ("VarTest-400", "VarTest-glyf", 400, None),
        ("VarTest-650", "VarTest-glyf", 650, {"wght": 650}),
        ("VarTest-900", "VarTest-glyf", 900, {"wght": 900}),
        ("VarTest-700-75", "VarTest-glyf", 700, {"wght": 700, "wdth": 75}),
        ("VarTest-cond", "VarTest-glyf", 400, {"wdth": 50}),
        ("VarCFF2-100", "VarTest-CFF2", 100, {"wght": 100}),
        ("VarCFF2-900", "VarTest-CFF2", 900, {"wght": 900}),
    ]
    fonts = list(common.FONTS)
    for fid, source, weight, variations in instances:
        family = "VarCFF2" if "CFF2" in source else "VarTest"
        decl = {"id": fid, "family": family, "weight": weight, "source": source}
        if variations:
            decl["variations"] = variations
        fonts.append(decl)
    lines = []
    for fid, _, _, variations in instances:
        label = ", ".join(f"{k} {v}" for k, v in (variations or {}).items()) or "default"
        lines.append(common.box(
            common.text(common.run(f"{fid} ({label})", size=8), width=150),
            common.text(common.run("HELLO HOTEL FILE TOE", font=fid, size=24)),
            flexDirection="row", alignItems="center", margin={"bottom": 8},
        ))
    # The stems of VarTest-opsz grow with the size, but for `opticalSizing: none`.
    fonts += [{"id": "Opsz", "family": "VarOpsz", "source": "VarTest-opsz"},
              {"id": "Opsz-fixed", "family": "VarOpsz", "source": "VarTest-opsz", "opticalSizing": "none"}]
    for fid, size in [("Opsz", 8), ("Opsz", 16), ("Opsz", 40), ("Opsz-fixed", 40)]:
        lines.append(common.box(
            common.text(common.run(f"{fid} at {size} pt", size=8), width=150),
            common.text(common.run("HOTEL FILE", font=fid, size=size)),
            flexDirection="row", alignItems="center", margin={"bottom": 8},
        ))
    # SVG text in a declaration's ranges draws at its weight and width.
    fonts.append({"id": "VarRange", "family": "VarRange", "source": "VarTest-glyf",
                  "weightRange": [100, 900], "stretchRange": [50, 100]})
    svg = ('<svg xmlns="http://www.w3.org/2000/svg" width="400" height="100">'
           '<text x="0" y="40" font-family="VarTest" font-weight="650" font-size="30">HOTEL</text>'
           '<text x="220" y="40" font-family="VarTest" font-weight="900" font-size="30">FOIL</text>'
           '<text x="0" y="90" font-family="VarRange" font-weight="300" font-size="30">HOTEL</text>'
           '<text x="140" y="90" font-family="VarRange" font-weight="800" font-stretch="condensed" '
           'font-size="30">HOTEL</text>'
           '<text x="280" y="90" style="font: condensed 900 30px VarRange">FOIL</text></svg>')
    import base64
    lines.append({"type": "image", "data": "data:image/svg+xml;base64," + base64.b64encode(svg.encode()).decode(),
                  "style": {"width": 400, "height": 100}})
    return {
        "version": 1,
        "meta": {"title": "Variable fonts", "lang": "en"},
        "fonts": fonts,
        "pages": [{"size": common.A4, "margin": common.edges(40), "children": lines}],
    }


def main():
    common.write("variable-fonts", document())
    outputs = {
        FONTS / "VarTest-glyf.ttf": glyf_font(),
        FONTS / "VarTest-CFF2.otf": cff2_font(),
        FONTS / "VarTest-layout.ttf": glyf_font(layout=True),
        FONTS / "VarTest-COLRv1.ttf": colr_font(),
        FONTS / "VarTest-opsz.ttf": glyf_font(opsz=True),
        FONTS / "limits" / "var-avar2.ttf": glyf_font(avar_version=2),
        FONTS / "limits" / "var-64-axes.ttf": glyf_font(extra_axes=62),
        FONTS / "limits" / "var-no-gvar.ttf": glyf_font(gvar_len=0),
        # Into its glyph offsets.
        FONTS / "limits" / "var-gvar-cut.ttf": glyf_font(gvar_len=24),
        FONTS / "limits" / "var-cff-and-cff2.otf": cff2_font(cff=True),
    }
    for path, data in outputs.items():
        path.write_bytes(data)
        print(f"{path.relative_to(ROOT)}: {len(data)} bytes")


if __name__ == "__main__":
    main()
