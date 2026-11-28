#!/usr/bin/env python3
"""Build every Svetra brand file from the mark's geometry. Standard library only.

    python3 brand/scripts/build.py            write brand/svg, brand/png and brand/favicon.ico
    python3 brand/scripts/build.py --svg-only just the SVGs (fast)
    python3 brand/scripts/build.py --check    fail if a committed file differs from a fresh build
"""
import sys
from pathlib import Path

import geometry as g
import image
import layout
import raster

ROOT = Path(__file__).resolve().parent.parent
BLACK, WHITE = (0, 0, 0), (255, 255, 255)
HEX = {BLACK: "#000", WHITE: "#fff"}

# Colours per variant: role -> rgb. "current" uses currentColor in SVG and is not rasterised.
ON_LIGHT = {"mark": BLACK, "text": BLACK}
ON_DARK = {"mark": WHITE, "text": WHITE}
TILE_BLACK = {"tile": BLACK, "mark": WHITE}
TILE_WHITE = {"tile": WHITE, "mark": BLACK}
CURRENT = {"mark": "current", "text": "current"}


def fill(c):
    return "currentColor" if c == "current" else HEX[c]


def svg(scene, colors, title="Svetra"):
    w, h, layers = scene
    paths = "".join(f'<path fill="{fill(colors[role])}" d="{g.svg_d(p)}"/>' for p, role in layers)
    n = lambda v: f"{v:.2f}".rstrip("0").rstrip(".")
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {n(w)} {n(h)}" role="img">'
            f"<title>{title}</title>{paths}</svg>\n")


def favicon_svg():
    w, h, layers = layout.mark_scene()
    d = g.svg_d(layers[0][0])
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:g} {h:g}">'
            "<title>Svetra</title>"
            "<style>path{fill:#000}@media (prefers-color-scheme:dark){path{fill:#fff}}</style>"
            f'<path d="{d}"/></svg>\n')


def render(scene, colors, width, height=None, pad=0.0, ground=None):
    """Draw a scene into a width x height canvas, scaled to fit inside the padding."""
    sw, sh, layers = scene
    height = height or round(width * sh / sw)
    s = min((width - 2 * pad) / sw, (height - 2 * pad) / sh)
    dx, dy = (width - sw * s) / 2, (height - sh * s) / 2
    canvas = raster.Canvas(width, height)
    if ground:
        canvas.paint(g.flatten(g.square(max(width, height) * 2)), ground)
    for path, role in layers:
        canvas.paint(g.flatten(g.move(path, s, dx, dy)), colors[role])
    return width, height, canvas.rgba()


def png(*args, **kw):
    w, h, rgba = render(*args, **kw)
    return image.png(w, h, rgba)


def outputs(svg_only=False):
    """name -> bytes for every file, paths relative to brand/."""
    out = {}
    scenes = {
        "mark": layout.mark_scene(),
        "wordmark": layout.wordmark_scene(),
        "lockup-horizontal": layout.lockup_horizontal(),
        "lockup-stacked": layout.lockup_stacked(),
    }
    for name, scene in scenes.items():
        for tone, colors in (("black", ON_LIGHT), ("white", ON_DARK), ("current", CURRENT)):
            out[f"svg/{name}-{tone}.svg"] = svg(scene, colors).encode()
    for tone, colors in (("black", TILE_BLACK), ("white", TILE_WHITE)):
        out[f"svg/icon-{tone}.svg"] = svg(layout.tile(), colors).encode()
    out["svg/favicon.svg"] = favicon_svg().encode()
    if svg_only:
        return out

    # Favicon-sized icons get a bigger disc, so the cut still reads at 16 px.
    mark, rounded, small = layout.mark_scene(), layout.tile(), layout.tile(True, 0.72)
    for size in (16, 32, 64, 128, 256, 512):
        out[f"png/mark-black-{size}.png"] = png(mark, ON_LIGHT, size)
        out[f"png/mark-white-{size}.png"] = png(mark, ON_DARK, size)
    for size in (16, 32, 48, 64, 128, 256, 512):
        out[f"png/icon-black-{size}.png"] = png(small if size <= 48 else rounded, TILE_BLACK, size)
    out["png/icon-white-512.png"] = png(rounded, TILE_WHITE, 512)
    out["png/apple-touch-icon.png"] = png(layout.tile(False, 0.62), TILE_BLACK, 180)
    for size in (192, 512):
        out[f"png/icon-maskable-{size}.png"] = png(layout.tile(False, 0.5), TILE_BLACK, size)
    out["png/avatar-500.png"] = png(layout.tile(False, 0.62), TILE_BLACK, 500)
    for tone, colors in (("black", ON_LIGHT), ("white", ON_DARK)):
        out[f"png/lockup-horizontal-{tone}-1024.png"] = png(layout.lockup_horizontal(), colors, 1024)
        out[f"png/lockup-stacked-{tone}-1024.png"] = png(layout.lockup_stacked(), colors, 1024)
    out["png/og-1200x630.png"] = png(layout.lockup_horizontal(), ON_DARK, 1200, 630, pad=190, ground=BLACK)
    out["favicon.ico"] = image.ico([(s, render(small, TILE_BLACK, s)[2]) for s in (16, 32, 48)])
    return out


def same(path, fresh):
    old = path.read_bytes()
    if path.suffix == ".png":
        try:
            return image.png_pixels(old) == image.png_pixels(fresh)
        except Exception:
            return False
    return old == fresh


def main(argv):
    flags = set(argv)
    unknown = flags - {"--svg-only", "--check"}
    if unknown:
        sys.exit(__doc__)
    files = outputs(svg_only="--svg-only" in flags)
    if "--check" in flags:
        bad = [n for n, data in files.items() if not (ROOT / n).exists() or not same(ROOT / n, data)]
        for n in bad:
            print(f"differs: brand/{n}")
        print(f"{len(files) - len(bad)} of {len(files)} files match")
        return 1 if bad else 0
    for name, data in files.items():
        path = ROOT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    print(f"wrote {len(files)} files under {ROOT}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
