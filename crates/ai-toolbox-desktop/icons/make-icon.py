#!/usr/bin/env python3
"""Draw the app icon: a toolbox mark on a rounded square.

Written out pixel by pixel rather than as an SVG because there is no rasteriser on a
stock macOS, and an icon that needs a tool nobody has is an icon that rots. Supersampled
4x and box-filtered down, which is what keeps the curves clean at 32px - the size that
actually has to read, in a dock and a menu bar.

    python3 make-icon.py icon.png [size]

The rest of the set comes from `npx @tauri-apps/cli@2 icon icon.png`.
"""
import math
import struct
import sys
import zlib

SS = 4  # supersampling factor

# The board's accent green, and the off-white it sits on.
BG_TOP = (0x35, 0x7C, 0x59)
BG_BOTTOM = (0x24, 0x5A, 0x40)
MARK = (0xFB, 0xFB, 0xFA)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def rounded_rect(x, y, w, h, r):
    """Signed coverage test for a rounded rectangle, in supersampled space."""

    def inside(px, py):
        dx = max(x - px, 0, px - (x + w))
        dy = max(y - py, 0, py - (y + h))
        if dx == 0 and dy == 0:
            return True
        # Corner radius only applies within the rounded region.
        cx = min(max(px, x + r), x + w - r)
        cy = min(max(py, y + r), y + h - r)
        return math.hypot(px - cx, py - cy) <= r

    return inside


def draw(size):
    n = size * SS
    # The toolbox: a body, a lid band, and a handle arc above it.
    pad = n * 0.20
    body_x, body_w = pad, n - 2 * pad
    body_y, body_h = n * 0.44, n * 0.30
    lid_y, lid_h = n * 0.355, n * 0.085
    handle_cx, handle_cy = n / 2, lid_y
    handle_r_outer, handle_r_inner = n * 0.145, n * 0.098

    body = rounded_rect(body_x, body_y, body_w, body_h, n * 0.035)
    lid = rounded_rect(body_x - n * 0.015, lid_y, body_w + n * 0.03, lid_h, n * 0.022)
    plate = rounded_rect(n * 0.43, n * 0.50, n * 0.14, n * 0.085, n * 0.018)
    badge = rounded_rect(0, 0, n, n, n * 0.22)

    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            r = g = b = a = 0
            for sy in range(SS):
                for sx in range(SS):
                    fx, fy = px * SS + sx + 0.5, py * SS + sy + 0.5
                    if not badge(fx, fy):
                        continue
                    shade = lerp(BG_TOP, BG_BOTTOM, fy / n)
                    colour = shade
                    on_mark = body(fx, fy) or lid(fx, fy)
                    if not on_mark:
                        d = math.hypot(fx - handle_cx, fy - handle_cy)
                        on_mark = handle_r_inner <= d <= handle_r_outer and fy <= handle_cy
                    if on_mark:
                        colour = MARK
                    # The latch plate is cut back out of the body, so the mark reads as
                    # a toolbox rather than a plain slab at small sizes.
                    if plate(fx, fy) and body(fx, fy):
                        colour = shade
                    r += colour[0]
                    g += colour[1]
                    b += colour[2]
                    a += 255
            samples = SS * SS
            if a == 0:
                row += bytes(4)
                continue
            # Premultiplied averaging would darken the edge against transparency, so the
            # colour is averaged over covered samples only and alpha carries the coverage.
            covered = a // 255
            row += bytes((r // covered, g // covered, b // covered, a // samples))
        rows.append(bytes(row))
    return rows


def write_png(path, rows, size):
    raw = b"".join(b"\x00" + row for row in rows)

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "icon.png"
    size = int(sys.argv[2]) if len(sys.argv) > 2 else 1024
    write_png(out, draw(size), size)
    print(f"wrote {out} at {size}x{size}")
