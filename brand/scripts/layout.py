"""Scenes: the mark, the wordmark, the lockups and the icon tiles, as outlines.

A scene is (width, height, layers); a layer is (path, role). Roles are
"mark", "text" and "tile"; build.py maps them to colours per variant.
"""
import json
from pathlib import Path

import geometry as g

WORDMARK = json.loads((Path(__file__).resolve().parent.parent / "src" / "wordmark.json").read_text())
TEXT = "svetra"
TRACKING = -0.03  # em, applied between letters
MARK_BOX = (4.0, 4.0, 60.0, 60.0)  # the disc inside the 64 grid


def _place_mark(diameter, x, y):
    s = diameter / 56.0
    return g.move(g.mark(), s, x - 4.0 * s, y - 4.0 * s)


def _text(px):
    """The wordmark at font size px, baseline at y=0, starting at x=0 (y down)."""
    k = px / WORDMARK["upm"]
    pen, path = 0.0, []
    for ch in TEXT:
        glyph = WORDMARK["glyphs"][ch]
        for seg in glyph["path"]:
            nums = seg[1:]
            moved = [(pen + v) * k if i % 2 == 0 else -v * k for i, v in enumerate(nums)]
            path.append((seg[0], *moved))
        pen += glyph["adv"] + TRACKING * WORDMARK["upm"]
    return path


def _shift(path, dx, dy):
    return g.move(path, 1.0, dx, dy)


def _fit(layers):
    """Translate layers so their joint bounding box starts at 0,0; return the scene."""
    boxes = [g.bounds(p) for p, _ in layers]
    x0, y0 = round(min(b[0] for b in boxes), 3), round(min(b[1] for b in boxes), 3)
    x1, y1 = round(max(b[2] for b in boxes), 3), round(max(b[3] for b in boxes), 3)
    return x1 - x0, y1 - y0, [(_shift(p, -x0, -y0), role) for p, role in layers]


def mark_scene():
    return _fit([(_place_mark(56.0, 0.0, 0.0), "mark")])


def wordmark_scene():
    return _fit([(_text(100.0), "text")])


def lockup_horizontal():
    d = 100.0
    px = d / 1.15
    x_height = WORDMARK["xHeight"] * px / WORDMARK["upm"]
    text = _shift(_text(px), d * 1.38, d / 2 + x_height / 2)
    return _fit([(_place_mark(d, 0.0, 0.0), "mark"), (text, "text")])


def lockup_stacked():
    d = 100.0
    text = _text(d * 0.62)
    tx0, ty0, tx1, _ = g.bounds(text)
    text = _shift(text, d / 2 - (tx0 + tx1) / 2, d * 1.30 - ty0)
    return _fit([(_place_mark(d, 0.0, 0.0), "mark"), (text, "text")])


def tile(rounded=True, disc=0.5625, size=64.0):
    d = size * disc
    ground = g.rounded_square(size, size * 0.225) if rounded else g.square(size)
    return size, size, [(ground, "tile"), (_place_mark(d, (size - d) / 2, (size - d) / 2), "mark")]
