#!/usr/bin/env python3
"""Compare an app screenshot with a design reference PNG.

Usage:
    python3 tools/design_diff.py docs/design/png/Performance.png target/design-check/Performance.png
    python3 tools/design_diff.py REF ACTUAL --out target/design-check/Performance

Writes <out>-side.png (design | app | heatmap) and prints:
  - the share of pixels that differ clearly (max channel delta > 48)
  - a 6x4 grid of the same share per region, so you know WHERE to look

The number is a guide, not a pass/fail on its own: fonts render slightly differently in a browser
and in Slint, so even a perfect match will not reach 0 %. Always look at the side-by-side image.
Needs Pillow (python-pillow on Arch, python3-pil on Ubuntu).
"""
import argparse
import sys

from PIL import Image, ImageChops, ImageFilter

THRESHOLD = 48
COLS, ROWS = 6, 4


def load(path, size=None):
    img = Image.open(path).convert("RGB")
    if size and img.size != size:
        img = img.resize(size, Image.LANCZOS)
    return img


def edges(img):
    """Binary map of edges (text, borders, chart lines)."""
    e = img.convert("L").filter(ImageFilter.FIND_EDGES)
    return e.point(lambda v: 255 if v > 40 else 0)


def layout_mismatch(ref, act, slack=5):
    """Share of edge pixels in either image with no edge nearby (within `slack` px) in the other.

    0 % = every edge lines up; 100 % = nothing lines up. Dark empty areas don't dilute it,
    which is why it is the main score.
    """
    er, ea = edges(ref), edges(act)
    grow = ImageFilter.MaxFilter(2 * slack + 1)
    missing = ImageChops.multiply(er, ImageChops.invert(ea.filter(grow)))
    extra = ImageChops.multiply(ea, ImageChops.invert(er.filter(grow)))
    n_ref, n_act = er.histogram()[255], ea.histogram()[255]
    if n_ref + n_act == 0:
        return 0.0
    return 100 * (missing.histogram()[255] + extra.histogram()[255]) / (n_ref + n_act)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("reference")
    ap.add_argument("actual")
    ap.add_argument("--out", help="prefix for output images (default: next to ACTUAL)")
    a = ap.parse_args()

    ref = load(a.reference)
    act_raw = Image.open(a.actual)
    resized = act_raw.size != ref.size
    act = load(a.actual, ref.size)

    diff = ImageChops.difference(ref, act)
    # One channel per pixel: the largest of the R, G, B differences.
    r, g, b = diff.split()
    mx = ImageChops.lighter(ImageChops.lighter(r, g), b)
    mask = mx.point(lambda v: 255 if v > THRESHOLD else 0)

    w, h = ref.size
    total = w * h
    bad = mask.histogram()[255]
    print(f"reference {w}x{h}, actual {act_raw.size[0]}x{act_raw.size[1]}" + ("  (actual was RESIZED: fix the window size)" if resized else ""))
    print(f"LAYOUT MISMATCH: {layout_mismatch(ref, act):.1f} %  (edges of text, boxes and lines that don't line up; main score)")
    print(f"colour difference: {100 * bad / total:.1f} % of pixels  (channel delta > {THRESHOLD})")
    print("by region (rows top to bottom, % different):")
    for row in range(ROWS):
        cells = []
        for col in range(COLS):
            box = (col * w // COLS, row * h // ROWS, (col + 1) * w // COLS, (row + 1) * h // ROWS)
            region = mask.crop(box)
            n = (box[2] - box[0]) * (box[3] - box[1])
            cells.append(f"{100 * region.histogram()[255] / n:5.1f}")
        print("  " + " ".join(cells))

    heat = Image.new("RGB", ref.size, (13, 15, 19))
    heat.paste((255, 80, 80), mask=mask)
    side = Image.new("RGB", (w * 3 + 32, h), (40, 40, 40))
    side.paste(ref, (0, 0))
    side.paste(act, (w + 16, 0))
    side.paste(Image.blend(ref, heat, 0.6), (2 * w + 32, 0))
    out = (a.out or a.actual.rsplit(".", 1)[0]) + "-side.png"
    side.save(out)
    print(f"side-by-side: {out}  (left: design, middle: app, right: differences in red)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
