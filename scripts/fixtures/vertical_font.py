#!/usr/bin/env python3
"""Generates fixtures/fonts/VertTest.ttf, a test font for vertical text,
written from scratch (no font tools, no third-party artwork), so it is
small and free of licences.

It has ideographs and kana drawn as boxes and bars on a 1000-unit em
(縦 書 文 字 テ ス ト), the punctuation that changes in vertical text
(、 。 「 」 ー) with their vertical forms, which `vert` substitutes, and
〈, which has none; Latin capitals and digits (A B C 1 2), which mixed
orientation turns sideways; and the spaces. `vhea` and `vmtx` give every
glyph a vertical advance of 1000, and `VORG` puts the vertical origin at
880, the top of the em box (-120 to 880).

The output is committed; scripts/check-generated.sh regenerates it and
fails when it differs, so edit this script, not the font.

  scripts/fixtures/vertical_font.py
"""
import pathlib
import struct
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import emoji_fonts as ef  # noqa: E402

ROOT = ef.ROOT
FONTS = ef.FONTS

# The em box: y from -120 to 880; the vertical origin is its top.
EM_TOP, EM_BOTTOM = 880, -120
VERTICAL_ADVANCE = 1000


def r(x0, y0, x1, y1):
    return ef.rect(x0, y0, x1, y1)


def hole(x0, y0, x1, y1):
    """A counter: a rectangle the other way round."""
    return list(reversed(ef.rect(x0, y0, x1, y1)))


def frame(*inside):
    """A square frame on the em box, with bars inside it."""
    return [r(100, -40, 900, 800), hole(160, 20, 840, 740), *inside]


# Name, code point (None: a vertical form), contours, horizontal advance.
GLYPHS = [
    (".notdef", None, [r(50, -100, 450, 700), hole(100, -50, 400, 650)], 500),
    ("space", 0x20, [], 300),
    ("ideographicspace", 0x3000, [], 1000),
    # 縦: a frame with a stem down the middle.
    ("uni7E26", 0x7E26, frame(r(470, 20, 530, 740)), 1000),
    # 書: a frame with three bars across.
    ("uni66F8", 0x66F8, frame(r(160, 200, 840, 250), r(160, 360, 840, 410), r(160, 520, 840, 570)), 1000),
    # 文: a cross.
    ("uni6587", 0x6587, [r(100, 520, 900, 600), r(460, -40, 540, 800)], 1000),
    # 字: a frame with a bar on top.
    ("uni5B57", 0x5B57, frame(r(160, 600, 840, 650)), 1000),
    # テ, ス, ト: bars.
    ("uni30C6", 0x30C6, [r(200, 640, 800, 700), r(120, 420, 880, 480), r(460, -20, 540, 420)], 1000),
    ("uni30B9", 0x30B9, [r(200, 640, 760, 700), r(700, 100, 760, 640), r(160, 60, 700, 120)], 1000),
    ("uni30C8", 0x30C8, [r(300, -40, 380, 780), r(380, 380, 760, 440)], 1000),
    # 、 and 。 at the bottom left; their vertical forms at the top right.
    ("uni3001", 0x3001, [r(120, -60, 260, 80)], 1000),
    ("uni3002", 0x3002, [r(100, -80, 300, 120), hole(150, -30, 250, 70)], 1000),
    ("uni3001.vert", None, [r(740, 620, 880, 760)], 1000),
    ("uni3002.vert", None, [r(700, 580, 900, 780), hole(750, 630, 850, 730)], 1000),
    # 「 and 」: corners; vertical forms turned a quarter clockwise.
    ("uni300C", 0x300C, [r(560, 200, 640, 820), r(640, 740, 900, 820)], 1000),
    ("uni300D", 0x300D, [r(360, -60, 440, 560), r(100, -60, 360, 20)], 1000),
    ("uni300C.vert", None, [r(180, 700, 800, 780), r(720, 440, 800, 700)], 1000),
    ("uni300D.vert", None, [r(200, -20, 820, 60), r(200, 60, 280, 300)], 1000),
    # ー: a bar across; its vertical form, a bar down.
    ("uni30FC", 0x30FC, [r(100, 340, 900, 420)], 1000),
    ("uni30FC.vert", None, [r(460, -20, 540, 780)], 1000),
    # 〈: no vertical form.
    ("uni3008", 0x3008, [r(560, -60, 640, 820), r(640, 340, 700, 420)], 1000),
    ("A", 0x41, [r(60, 0, 140, 700), r(460, 0, 540, 700), r(140, 620, 460, 700), r(140, 300, 460, 380)], 600),
    ("B", 0x42, [r(60, 0, 140, 700), r(140, 0, 500, 80), r(140, 310, 480, 390), r(140, 620, 480, 700), r(480, 80, 540, 620)], 600),
    ("C", 0x43, [r(60, 0, 140, 700), r(140, 0, 520, 80), r(140, 620, 520, 700)], 600),
    ("one", 0x31, [r(220, 0, 300, 700)], 500),
    ("two", 0x32, [r(60, 620, 440, 700), r(360, 350, 440, 620), r(60, 310, 440, 390), r(60, 80, 140, 310), r(60, 0, 440, 80)], 500),
]

VERT = {"uni3001": "uni3001.vert", "uni3002": "uni3002.vert", "uni300C": "uni300C.vert",
        "uni300D": "uni300D.vert", "uni30FC": "uni30FC.vert"}


def vhea(count):
    return struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00011000, 500, -500, 0, VERTICAL_ADVANCE, 0, 0, VERTICAL_ADVANCE,
        1, 0, 0, 0, 0, 0, 0, 0, count,
    )


def vmtx(glyphs):
    """Every glyph advances 1000 down; its top side bearing is from the
    top of the em box."""
    out = b""
    for g in glyphs:
        top = g.bounds()[3] if g.contours else EM_TOP
        out += struct.pack(">Hh", VERTICAL_ADVANCE, EM_TOP - top)
    return out


def vorg():
    return struct.pack(">HHhH", 1, 0, EM_TOP, 0)


def gsub_vert(pairs):
    """`vert` in the default script: one single-substitution lookup.
    `pairs`: [(glyph, vertical form)]."""
    pairs = sorted(pairs)
    cov = ef.coverage([a for a, _ in pairs])
    head_len = 6 + 2 * len(pairs)
    subtable = struct.pack(">HHH", 2, head_len, len(pairs)) + b"".join(struct.pack(">H", b) for _, b in pairs) + cov
    lookup = struct.pack(">HHHH", 1, 0, 1, 8) + subtable
    lookup_list = struct.pack(">HH", 1, 4) + lookup
    feature = struct.pack(">HHH", 0, 1, 0)
    feature_list = struct.pack(">H4sH", 1, b"vert", 8) + feature
    langsys = struct.pack(">HHHH", 0, 0xFFFF, 1, 0)
    script = struct.pack(">HH", 4, 0) + langsys
    script_list = struct.pack(">H4sH", 1, b"DFLT", 8) + script
    off_script = 10
    off_feature = off_script + len(script_list)
    off_lookup = off_feature + len(feature_list)
    return struct.pack(">IHHH", 0x00010000, off_script, off_feature, off_lookup) + script_list + feature_list + lookup_list


def font():
    glyphs = [ef.Glyph(name, contours, advance) for name, _, contours, advance in GLYPHS]
    gid = {g.name: i for i, g in enumerate(glyphs)}
    mapping = {cp: gid[name] for name, cp, _, _ in GLYPHS if cp is not None}
    extra = {
        "vhea": vhea(len(glyphs)),
        "vmtx": vmtx(glyphs),
        "VORG": vorg(),
        "GSUB": gsub_vert([(gid[a], gid[b]) for a, b in VERT.items()]),
    }
    return ef.outline_font("VertTest", glyphs, mapping, extra)


def main():
    path = FONTS / "VertTest.ttf"
    data = font()
    path.write_bytes(data)
    print(f"{path.relative_to(ROOT)}: {len(data)} bytes")


if __name__ == "__main__":
    main()
