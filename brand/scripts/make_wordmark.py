#!/usr/bin/env python3
"""Extract the outlines of the letters in "svetra" from a font into src/wordmark.json.

Run once, or when the wordmark changes. needs fontTools (pip install fonttools).
build.py reads the JSON, so building the brand files needs no font and no packages.

    python3 brand/scripts/make_wordmark.py path/to/Geist-SemiBold.ttf

The committed outlines come from Geist SemiBold (SIL Open Font License 1.1,
https://github.com/vercel/geist-font).
"""
import json
import sys
from pathlib import Path

from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTFont

TEXT = "svetra"
OUT = Path(__file__).resolve().parent.parent / "src" / "wordmark.json"


class Outline(BasePen):
    def __init__(self, glyphs):
        super().__init__(glyphs)
        self.path = []

    def _moveTo(self, p):
        self.path.append(["M", *p])

    def _lineTo(self, p):
        self.path.append(["L", *p])

    def _curveToOne(self, a, b, p):
        self.path.append(["C", *a, *b, *p])

    def _qCurveToOne(self, a, p):
        self.path.append(["Q", *a, *p])

    def _closePath(self):
        self.path.append(["Z"])


def r(seg):
    return [seg[0]] + [round(v, 1) for v in seg[1:]]


def main(font_path):
    font = TTFont(font_path)
    glyphs, cmap, hmtx = font.getGlyphSet(), font.getBestCmap(), font["hmtx"]
    out = {
        "font": font["name"].getDebugName(4),
        "license": "SIL Open Font License 1.1",
        "upm": font["head"].unitsPerEm,
        "xHeight": font["OS/2"].sxHeight,
        "glyphs": {},
    }
    for ch in sorted(set(TEXT)):
        name = cmap[ord(ch)]
        pen = Outline(glyphs)
        glyphs[name].draw(pen)
        out["glyphs"][ch] = {"adv": hmtx[name][0], "path": [r(s) for s in pen.path]}
    OUT.write_text(json.dumps(out, separators=(",", ":"), sort_keys=True) + "\n")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
