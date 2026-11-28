"""A small anti-aliased polygon rasteriser, standard library only.

It exists so every PNG is drawn from the same outlines as the SVGs, with no
image library and the same bytes on every machine.
"""


def coverage(contours, w, h, ss=4):
    """Fraction of each pixel covered by the polygons (non-zero fill rule).

    Returns h rows of w floats in 0..1. Each pixel row is sampled at ss
    sub-rows; along a sub-row the coverage is exact.
    """
    edges = []
    for c in contours:
        n = len(c)
        for i in range(n):
            xa, ya = c[i]
            xb, yb = c[(i + 1) % n]
            if ya == yb:
                continue
            if ya < yb:
                edges.append((ya, yb, xa, (xb - xa) / (yb - ya), 1))
            else:
                edges.append((yb, ya, xb, (xa - xb) / (ya - yb), -1))
    edges.sort()
    rows = []
    nxt, active = 0, []
    unit = 1.0 / ss
    for py in range(h):
        cov = [0.0] * (w + 2)
        diff = [0.0] * (w + 2)
        for sub in range(ss):
            y = py + (sub + 0.5) / ss
            while nxt < len(edges) and edges[nxt][0] <= y:
                active.append(edges[nxt])
                nxt += 1
            active = [e for e in active if e[1] > y]
            xs = sorted((e[2] + e[3] * (y - e[0]), e[4]) for e in active)
            wind = 0
            for k in range(len(xs) - 1):
                wind += xs[k][1]
                if wind == 0:
                    continue
                xa, xb = max(xs[k][0], 0.0), min(xs[k + 1][0], float(w))
                if xb <= xa:
                    continue
                ia, ib = int(xa), int(xb)
                if ia == ib:
                    cov[ia] += (xb - xa) * unit
                else:
                    cov[ia] += (ia + 1 - xa) * unit
                    cov[ib] += (xb - ib) * unit
                    diff[ia + 1] += unit
                    diff[ib] -= unit
        run = 0.0
        row = []
        for i in range(w):
            run += diff[i]
            v = cov[i] + run
            row.append(1.0 if v > 1.0 else (0.0 if v < 0.0 else v))
        rows.append(row)
    return rows


class Canvas:
    """RGBA canvas. Shapes are painted in order, source-over."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        n = w * h
        self.r, self.g, self.b, self.a = [0.0] * n, [0.0] * n, [0.0] * n, [0.0] * n

    def paint(self, contours, rgb):
        w = self.w
        sr, sg, sb = (v / 255.0 for v in rgb)
        for y, row in enumerate(coverage(contours, w, self.h)):
            base = y * w
            for x, c in enumerate(row):
                if c <= 0.0:
                    continue
                i = base + x
                k = 1.0 - c
                self.r[i] = sr * c + self.r[i] * k
                self.g[i] = sg * c + self.g[i] * k
                self.b[i] = sb * c + self.b[i] * k
                self.a[i] = c + self.a[i] * k

    def rgba(self):
        out = bytearray()
        for r, g, b, a in zip(self.r, self.g, self.b, self.a):
            if a <= 0.0:
                out += b"\x00\x00\x00\x00"
            else:
                out += bytes((round(min(1.0, r / a) * 255), round(min(1.0, g / a) * 255),
                              round(min(1.0, b / a) * 255), round(a * 255)))
        return bytes(out)
