"""The Svetra mark: a disc cut by an S-shaped band that runs off both sides.

The mark is defined once, here, as two closed outlines on a 64 x 64 grid
(disc centre 32,32, radius 28). Everything else in brand/ is derived from it.

A path is a list of tuples:
  ("M", x, y)  ("L", x, y)  ("A", r, large, sweep, x, y)
  ("Q", x1, y1, x, y)  ("C", x1, y1, x2, y2, x, y)  ("Z",)
"""
from math import acos, atan2, cos, pi, sin, sqrt

CX = CY = 32.0
R = 28.0  # disc radius
HALF = 4.0  # half the width of the cut
ARC = 9.0  # radius of the S's two turns, measured on its centre line
TURN_A = 27.0  # x of the upper turn (bulges left)
TURN_B = 37.0  # x of the lower turn (bulges right)
ROW_TOP, ROW_MID, ROW_BOT = 14.0, 32.0, 50.0  # y of the centre line's three bars


def _rim_x(y, right):
    """x where the horizontal line at y meets the disc, on the right or left."""
    dx = sqrt(R * R - (y - CY) ** 2)
    return CX + dx if right else CX - dx


def mark():
    """The two pieces of the disc that the cut leaves behind."""
    outer, inner = ARC + HALF, ARC - HALF
    top_out, top_in = ROW_TOP - HALF, ROW_TOP + HALF
    mid_top, mid_bot = ROW_MID - HALF, ROW_MID + HALF
    bot_in, bot_out = ROW_BOT - HALF, ROW_BOT + HALF
    upper = [
        ("M", _rim_x(top_out, True), top_out),
        ("L", TURN_A, top_out),
        ("A", outer, 0, 0, TURN_A, mid_bot),
        ("L", TURN_B, mid_bot),
        ("A", inner, 0, 1, TURN_B, bot_in),
        ("L", _rim_x(bot_in, False), bot_in),
        ("A", R, 0, 1, _rim_x(top_out, True), top_out),
        ("Z",),
    ]
    lower = [
        ("M", _rim_x(top_in, True), top_in),
        ("L", TURN_A, top_in),
        ("A", inner, 0, 0, TURN_A, mid_top),
        ("L", TURN_B, mid_top),
        ("A", outer, 0, 1, TURN_B, bot_out),
        ("L", _rim_x(bot_out, False), bot_out),
        ("A", R, 0, 0, _rim_x(top_in, True), top_in),
        ("Z",),
    ]
    return upper + lower


def rounded_square(size, radius):
    s, r = size, radius
    return [
        ("M", r, 0), ("L", s - r, 0), ("A", r, 0, 1, s, r),
        ("L", s, s - r), ("A", r, 0, 1, s - r, s),
        ("L", r, s), ("A", r, 0, 1, 0, s - r),
        ("L", 0, r), ("A", r, 0, 1, r, 0), ("Z",),
    ]


def square(size):
    return [("M", 0, 0), ("L", size, 0), ("L", size, size), ("L", 0, size), ("Z",)]


def move(path, scale, dx, dy):
    """Uniform scale, then translate. Safe for arcs."""
    out = []
    for seg in path:
        c = seg[0]
        if c == "A":
            _, r, large, sweep, x, y = seg
            out.append((c, r * scale, large, sweep, x * scale + dx, y * scale + dy))
        elif c == "Z":
            out.append(seg)
        else:
            nums = seg[1:]
            out.append((c, *[(v * scale + (dx if i % 2 == 0 else dy)) for i, v in enumerate(nums)]))
    return out


def _n(v):
    t = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if t in ("-0", "") else t


def svg_d(path):
    parts = []
    for seg in path:
        c = seg[0]
        if c == "A":
            _, r, large, sweep, x, y = seg
            parts.append(f"A{_n(r)} {_n(r)} 0 {large} {sweep} {_n(x)} {_n(y)}")
        elif c == "Z":
            parts.append("Z")
        else:
            parts.append(c + " ".join(_n(v) for v in seg[1:]))
    return "".join(parts)


def _arc_points(p0, seg, tol):
    _, r, large, sweep, x1, y1 = seg
    x0, y0 = p0
    dx, dy = (x0 - x1) / 2, (y0 - y1) / 2
    d2 = dx * dx + dy * dy
    if d2 == 0:
        return []
    if d2 > r * r:
        r = sqrt(d2)
    coef = sqrt(max(0.0, (r * r - d2) / d2))
    sign = -1 if large == sweep else 1
    cx = sign * coef * dy + (x0 + x1) / 2
    cy = -sign * coef * dx + (y0 + y1) / 2
    a0, a1 = atan2(y0 - cy, x0 - cx), atan2(y1 - cy, x1 - cx)
    da = a1 - a0
    if sweep and da < 0:
        da += 2 * pi
    if not sweep and da > 0:
        da -= 2 * pi
    step = 2 * acos(max(0.0, 1 - tol / r))
    n = max(2, int(abs(da) / step) + 1)
    return [(cx + r * cos(a0 + da * i / n), cy + r * sin(a0 + da * i / n)) for i in range(1, n + 1)]


def flatten(path, tol=0.02):
    """Path to a list of closed polygons (lists of points)."""
    contours, cur, p = [], [], (0.0, 0.0)
    for seg in path:
        c = seg[0]
        if c == "M":
            if cur:
                contours.append(cur)
            p = (seg[1], seg[2])
            cur = [p]
        elif c == "L":
            p = (seg[1], seg[2])
            cur.append(p)
        elif c == "A":
            cur.extend(_arc_points(p, seg, tol))
            p = (seg[4], seg[5])
        elif c == "Q":
            _, x1, y1, x, y = seg
            for i in range(1, 13):
                t = i / 12
                u = 1 - t
                cur.append((u * u * p[0] + 2 * u * t * x1 + t * t * x, u * u * p[1] + 2 * u * t * y1 + t * t * y))
            p = (x, y)
        elif c == "C":
            _, x1, y1, x2, y2, x, y = seg
            for i in range(1, 17):
                t = i / 16
                u = 1 - t
                cur.append((u**3 * p[0] + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t**3 * x,
                            u**3 * p[1] + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t**3 * y))
            p = (x, y)
        elif c == "Z":
            if cur:
                contours.append(cur)
                p = cur[0]
                cur = []
    if cur:
        contours.append(cur)
    return contours


def bounds(path):
    pts = [pt for c in flatten(path) for pt in c]
    xs, ys = [x for x, _ in pts], [y for _, y in pts]
    return min(xs), min(ys), max(xs), max(ys)
