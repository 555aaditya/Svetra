# Svetra brand

The logo, drawn once and built into every file we need. Black and white only.

## The mark

A disc cut by an S. The cut is a band that runs off both sides of the disc, so the disc falls into two pieces and the S is the gap between them. It is the idea of the project drawn literally: everything closed by default, with one deliberate opening.

Geometry, on a 64 × 64 grid (all in `scripts/geometry.py`):

| Part | Value |
|---|---|
| Disc | centre 32, 32; radius 28 |
| Cut | a band 8 wide (half width 4) along the S's centre line |
| Centre line | three horizontal bars at y = 14, 32, 50, joined by two turns of radius 9 at x = 27 (bulging left) and x = 37 (bulging right) |
| Ends | the top bar runs off the right of the disc and the bottom bar off the left |

## The wordmark

`svetra`, lowercase, in Geist SemiBold with -0.03 em tracking, stored as outlines in `src/wordmark.json`. Geist is under the SIL Open Font License 1.1 (<https://github.com/vercel/geist-font>). The outlines are shipped, not the font, so the build needs no font file.

## Files

SVGs are in `svg/`. Each mark or lockup comes in `-black` (for light backgrounds), `-white` (for dark backgrounds) and `-current` (takes the text colour of the page).

| File | Use |
|---|---|
| `svg/mark-*.svg` | the mark alone, 56 × 56, no padding |
| `svg/wordmark-*.svg` | the word alone |
| `svg/lockup-horizontal-*.svg` | mark and word side by side: site header, README, docs |
| `svg/lockup-stacked-*.svg` | mark above the word: splash screens, slides, posters |
| `svg/icon-black.svg`, `svg/icon-white.svg` | the mark on a rounded square: app icon, avatar |
| `svg/favicon.svg` | the mark alone; black on a light tab, white on a dark one |
| `favicon.ico` | 16, 32 and 48 px, black rounded square, for browsers without SVG favicons |
| `png/mark-{black,white}-{16…512}.png` | transparent PNGs of the mark |
| `png/icon-black-{16…512}.png`, `png/icon-white-512.png` | rounded-square icons; 16 to 48 px use a larger disc so the cut still reads |
| `png/apple-touch-icon.png` | 180 px, full-bleed (iOS rounds the corners itself) |
| `png/icon-maskable-{192,512}.png` | full-bleed, mark inside the safe zone, for web app manifests |
| `png/avatar-500.png` | GitHub organization and npm avatar |
| `png/lockup-{horizontal,stacked}-{black,white}-1024.png` | transparent lockups |
| `png/og-1200x630.png` | social preview image: white lockup on black |

For a web page:

```html
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
```

## Rules

- Black on white or white on black. No other colours, no gradients, no outlines, no effects.
- Keep clear space around a mark or lockup of at least a quarter of the disc's diameter.
- Smallest sizes: 16 px for the mark, 24 px tall for a lockup.
- Do not rotate, stretch, re-space or redraw the mark. Use the files.
- Do not set the word in another font. Use the wordmark or lockup files.

## Rebuilding

The build uses only the Python 3 standard library. It draws the SVGs from the geometry and rasterises the PNGs and the ICO from the same outlines, so every file comes from one definition.

```sh
python3 brand/scripts/build.py              # write everything
python3 brand/scripts/build.py --svg-only   # SVGs only, instant
python3 brand/scripts/build.py --check      # exit 1 if a committed file is out of date
```

| Script | Role |
|---|---|
| `scripts/geometry.py` | the mark's outlines, path maths, flattening to polygons |
| `scripts/layout.py` | the mark, wordmark, lockups and icon tiles as scenes |
| `scripts/raster.py` | an anti-aliased polygon rasteriser |
| `scripts/image.py` | PNG and ICO writers |
| `scripts/build.py` | builds every file |
| `scripts/make_wordmark.py` | re-extracts the letters from a font into `src/wordmark.json`; needs `fonttools`, and only when the wordmark changes |

To change the mark, edit `scripts/geometry.py` and run the build. To change what is produced, edit `outputs()` in `scripts/build.py`.
