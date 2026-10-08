#!/usr/bin/env python3
"""Visual regression: rasterise fixture PDFs and compare with references.

  scripts/visual-diff.py            # compare (fails on differences)
  scripts/visual-diff.py --update   # regenerate fixtures/expected/

Inputs are the PDFs written by `cargo test --test fixtures` into
target/fixtures/. Pages are rasterised with `pdftoppm -r 72`. A page fails
when more than 0.1 % of its pixels differ by more than 24/255 in any channel
(antialiasing varies slightly between poppler builds).

For long generated fixtures only the first MAX_PAGES pages are stored, plus
the page count in `<name>.pages`.
"""
import pathlib
import shutil
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
PDFS = ROOT / "target/fixtures"
EXPECTED = ROOT / "fixtures/expected"
OUT = ROOT / "target/visual"
MAX_PAGES = 4
THRESHOLD = 0.001
CHANNEL_TOLERANCE = 24


def rasterise(pdf: pathlib.Path, dest: pathlib.Path) -> list:
    dest.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["pdftoppm", "-png", "-r", "72", "-l", str(MAX_PAGES), str(pdf), str(dest / pdf.stem)],
        check=True,
    )
    pages = sorted(dest.glob(f"{pdf.stem}-*.png"), key=lambda p: int(p.stem.rsplit("-", 1)[1]))
    # Normalise names to <stem>-<n>.png (pdftoppm zero-pads for long docs).
    out = []
    for p in pages:
        n = int(p.stem.rsplit("-", 1)[1])
        target = dest / f"{pdf.stem}-{n}.png"
        if p != target:
            p.rename(target)
        out.append(target)
    return out


def page_count(pdf: pathlib.Path) -> int:
    info = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True, check=True).stdout
    for line in info.splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    raise RuntimeError(f"no page count for {pdf}")


def diff_ratio(a: pathlib.Path, b: pathlib.Path, diff_out: pathlib.Path):
    ia = np.asarray(Image.open(a).convert("RGB"), dtype=np.int16)
    ib = np.asarray(Image.open(b).convert("RGB"), dtype=np.int16)
    if ia.shape != ib.shape:
        return 1.0
    mask = (np.abs(ia - ib) > CHANNEL_TOLERANCE).any(axis=2)
    ratio = mask.mean()
    if ratio > 0:
        vis = (ib * 0.3).astype(np.uint8)
        vis[mask] = [255, 0, 0]
        Image.fromarray(vis).save(diff_out)
    return ratio


def main() -> int:
    update = "--update" in sys.argv
    pdfs = sorted(PDFS.glob("*.pdf"))
    if not pdfs:
        print("no PDFs in target/fixtures; run `cargo test --test fixtures` first")
        return 1
    if shutil.which("pdftoppm") is None:
        print("pdftoppm not installed; skipped")
        return 0
    if OUT.exists():
        shutil.rmtree(OUT)
    failures = 0
    for pdf in pdfs:
        name = pdf.stem
        count = page_count(pdf)
        with tempfile.TemporaryDirectory() as tmp:
            pages = rasterise(pdf, pathlib.Path(tmp))
            if update:
                for old in EXPECTED.glob(f"{name}-*.png"):
                    old.unlink()
                EXPECTED.mkdir(parents=True, exist_ok=True)
                for p in pages:
                    shutil.copy(p, EXPECTED / p.name)
                (EXPECTED / f"{name}.pages").write_text(f"{count}\n")
                print(f"{name}: {count} page(s), stored {len(pages)}")
                continue

            count_file = EXPECTED / f"{name}.pages"
            if not count_file.exists():
                print(f"FAIL {name}: no reference images (run scripts/visual-diff.py --update and review)")
                failures += 1
                continue
            want = int(count_file.read_text())
            if want != count:
                print(f"FAIL {name}: {count} pages, expected {want}")
                failures += 1
            for p in pages:
                ref = EXPECTED / p.name
                if not ref.exists():
                    print(f"FAIL {name}: missing reference {ref.name}")
                    failures += 1
                    continue
                OUT.mkdir(parents=True, exist_ok=True)
                ratio = diff_ratio(p, ref, OUT / f"{p.stem}-diff.png")
                if ratio > THRESHOLD:
                    shutil.copy(p, OUT / p.name)
                    print(f"FAIL {p.stem}: {ratio * 100:.3f}% pixels differ (see target/visual/{p.stem}-diff.png)")
                    failures += 1
            if failures == 0:
                print(f"ok   {name} ({count} page(s))")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
