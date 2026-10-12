#!/usr/bin/env python3
"""Generates fixtures/images/* (deterministic, Pillow) and
fixtures/docs/images-jpeg-png.json."""
import base64, io, os

from PIL import Image, ImageDraw

from common import A4, ROOT, edges, write

IMG = os.path.join(ROOT, "fixtures", "images")
os.makedirs(IMG, exist_ok=True)


def gradient(w, h):
    im = Image.new("RGB", (w, h))
    px = im.load()
    for y in range(h):
        for x in range(w):
            px[x, y] = (int(255 * x / (w - 1)), int(255 * y / (h - 1)), 160)
    d = ImageDraw.Draw(im)
    d.rectangle([w // 10, h // 10, w // 3, h // 3], fill=(20, 20, 20))
    d.ellipse([w // 2, h // 3, w - w // 8, h - h // 8], fill=(250, 250, 250))
    return im


photo = gradient(320, 200)
photo.save(os.path.join(IMG, "photo.jpg"), quality=85, optimize=False, progressive=False)

# Stored landscape, EXIF orientation 6 (display = rotate 90° clockwise):
# displayed portrait with the dark square at the top-right.
exif = Image.Exif()
exif[0x0112] = 6
photo.save(os.path.join(IMG, "photo-rot6.jpg"), quality=85, exif=exif.tobytes())

photo.convert("CMYK").save(os.path.join(IMG, "photo-cmyk.jpg"), quality=85)
photo.convert("L").save(os.path.join(IMG, "photo-gray.jpg"), quality=85)

logo = Image.new("RGBA", (160, 160), (0, 0, 0, 0))
d = ImageDraw.Draw(logo)
d.ellipse([8, 8, 152, 152], fill=(37, 99, 235, 255))
d.rectangle([50, 50, 110, 110], fill=(255, 255, 255, 128))
logo.save(os.path.join(IMG, "logo.png"))

gray = Image.new("LA", (64, 64))
for y in range(64):
    for x in range(64):
        gray.putpixel((x, y), (x * 4, 255 if (x // 8 + y // 8) % 2 else 90))
gray.save(os.path.join(IMG, "checker-la.png"))

pal = Image.new("P", (48, 32))
pal.putpalette([255, 0, 0, 0, 160, 0, 0, 0, 255, 250, 200, 0])
for y in range(32):
    for x in range(48):
        pal.putpixel((x, y), (x // 12) % 4)
pal.save(os.path.join(IMG, "palette.png"))

buf = io.BytesIO()
Image.new("RGB", (8, 8), (220, 38, 38)).save(buf, format="PNG")
inline = base64.b64encode(buf.getvalue()).decode()

BLACK = [0, 0, 0, 1]


def text(s, size=9, **style):
    return {"type": "text", "style": style, "runs": [{"text": s, "font": "Inter-Regular", "size": size, "color": BLACK}]}


def cell(label, img_node):
    return {
        "type": "box",
        "style": {"width": 150, "gap": 4},
        "children": [text(label), img_node],
    }


FRAME = {"width": 150, "height": 90, "backgroundColor": [0.93, 0.93, 0.93, 1]}


def img(src, fit="fill", **style):
    """An image node; without `src` (None) for one that takes inline `data`."""
    s = dict(FRAME)
    s.update(style)
    return {"type": "image", **({"src": src} if src else {}), "fit": fit, "style": s}


row = lambda *c: {"type": "box", "style": {"flexDirection": "row", "gap": 16, "margin": {"bottom": 16}}, "children": list(c)}

doc = {
    "version": 1,
    "meta": {"title": "Images"},
    "fonts": [{"id": "Inter-Regular", "family": "Inter", "weight": 400, "style": "normal"}],
    "pages": [
        {
            "size": A4,
            "margin": edges(40),
            "children": [
                text("Images: JPEG passthrough, PNG with alpha, fit modes", 14, margin={"bottom": 12}),
                row(
                    cell("fill", img("photo", "fill")),
                    cell("contain", img("photo", "contain")),
                    cell("cover", img("photo", "cover")),
                ),
                row(
                    cell("none", img("photo", "none")),
                    cell("EXIF orientation 6", img("photo-rot6", "contain")),
                    cell("CMYK (Adobe)", img("photo-cmyk", "contain")),
                ),
                row(
                    cell("grayscale JPEG", img("photo-gray", "contain")),
                    cell("PNG alpha on colour", img("logo", "contain", backgroundColor=[0.98, 0.85, 0.3, 1])),
                    cell("LA + palette PNG", {
                        "type": "box",
                        "style": {"flexDirection": "row", "gap": 8},
                        "children": [
                            {"type": "image", "src": "checker-la", "style": {"width": 64, "backgroundColor": [0.2, 0.6, 0.3, 1]}},
                            {"type": "image", "src": "palette"},
                        ],
                    }),
                ),
                row(
                    cell("intrinsic size (96 dpi)", {"type": "image", "src": "logo", "style": {}}),
                    cell("opacity 0.5, rounded clip", img("photo", "cover", opacity=0.5, borderRadius=16, overflow="hidden")),
                    cell("inline base64 + border", img(None, "fill", width=40, height=40, borderWidth=edges(2), borderColor=edges(BLACK), padding=edges(4)) | {"data": inline}),
                ),
                text("Width only (height from aspect ratio):", margin={"bottom": 4}),
                {"type": "image", "src": "photo", "style": {"width": 200}},
            ],
        }
    ],
}
write("images-jpeg-png", doc)
