#!/usr/bin/env python3
"""tools/mc/hud/ref.py -- independent reference for lib/mc/mc_hud.fi.

Replays <dir>/scenes.txt (the operations dump.fi executed, in order) on the same base pattern with its own code
(python integers, the font is read straight from font5x7.txt, number formatting with python ints) and compares every
framebuffer <dir>/<scene>.rgba byte for byte plus the SHA-256 printed by Firn. Written from docs/mc/hud.md.
Usage: ref.py <dir>   (one line per scene, exit 1 on any difference)
"""
import hashlib
import math
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def load_font():
    font = {}
    for line in open(os.path.join(HERE, "font5x7.txt")):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        p = line.split()
        font[int(p[0])] = [[c == "#" for c in row] for row in p[1:]]
    return font


FONT = load_font()


class Img:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [[0] * w for _ in range(h)]  # (r, g, b) tuples
        for y in range(h):
            for x in range(w):
                self.px[y][x] = ((x * 5 + y * 3) & 255, ((x * 7) ^ (y * 11)) & 255, (x * y) & 255)

    def rect(self, x, y, w, h, c):
        a = c >> 24
        if a == 0:
            return
        sr, sg, sb = c & 255, (c >> 8) & 255, (c >> 16) & 255
        for yy in range(max(y, 0), min(y + h, self.h)):
            for xx in range(max(x, 0), min(x + w, self.w)):
                if a == 255:
                    self.px[yy][xx] = (sr, sg, sb)
                else:
                    d = self.px[yy][xx]
                    self.px[yy][xx] = tuple((s * a + dd * (255 - a)) // 255 for s, dd in zip((sr, sg, sb), d))

    def invert(self, x, y, w, h):
        for yy in range(max(y, 0), min(y + h, self.h)):
            for xx in range(max(x, 0), min(x + w, self.w)):
                self.px[yy][xx] = tuple(255 - v for v in self.px[yy][xx])

    def glyph(self, x, y, s, c, code):
        if code < 32 or code > 126:
            code = 63
        for row in range(7):
            for col in range(5):
                if FONT[code][row][col]:
                    self.rect(x + col * s, y + row * s, s, s, c)

    def text(self, x, y, s, c, shadow, data):
        if shadow >> 24:
            for i, ch in enumerate(data):
                self.glyph(x + s + i * 6 * s, y + s, s, shadow, ch)
        for i, ch in enumerate(data):
            self.glyph(x + i * 6 * s, y, s, c, ch)

    def crosshair(self, s):
        cx, cy, t, l = self.w // 2, self.h // 2, s, 4 * s
        self.invert(cx - l - t // 2, cy - t // 2, 2 * l + t, t)
        self.invert(cx - t // 2, cy - t // 2 - l, t, l)
        self.invert(cx - t // 2, cy - t // 2 + t, t, l)

    def hotbar(self, s, slot):
        slot = max(0, min(8, slot))
        bw = 181 * s
        x0 = int((self.w - bw) / 2)  # Firn integer division truncates towards zero (negative when the bar is wider than the frame)
        y0 = self.h - 23 * s
        self.rect(x0, y0, bw, 22 * s, 0x78000000)
        self.rect(x0, y0, bw, s, 0xFF8B8B8B)
        self.rect(x0, y0 + 21 * s, bw, s, 0xFF8B8B8B)
        for k in range(10):
            self.rect(x0 + k * 20 * s, y0 + s, s, 20 * s, 0xFF8B8B8B)
        for i in range(9):
            col = 0xFFFFFFFF if i == slot else 0xFFAAAAAA
            self.glyph(x0 + i * 20 * s + 8 * s, y0 + s + (13 * s) // 2, s, col, 49 + i)
        xs, ys = x0 + slot * 20 * s - 2 * s, y0 - s
        self.rect(xs, ys, 24 * s, 2 * s, 0xFFFFFFFF)
        self.rect(xs, ys + 22 * s, 24 * s, 2 * s, 0xFFFFFFFF)
        self.rect(xs, ys + 2 * s, 2 * s, 20 * s, 0xFFFFFFFF)
        self.rect(xs + 22 * s, ys + 2 * s, 2 * s, 20 * s, 0xFFFFFFFF)

    def debug(self, s, d):
        for k, line in enumerate(debug_lines(d)):
            data = line.encode("ascii")
            x, y = 2 * s, 2 * s + k * 10 * s
            width = (len(data) * 6 - 1) * s if data else 0
            self.rect(x - s, y - s, width + 3 * s, 9 * s, 0x78000000)
            self.text(x, y, s, 0xFFFFFFFF, 0xFF3F3F3F, data)

    def bytes(self):
        out = bytearray()
        for row in self.px:
            for (r, g, b) in row:
                out += bytes((r, g, b, 255))
        return bytes(out)


def fixed(v, dec):
    if not (v < 1e12 and v > -1e12):
        return "?"
    a = -v if v < 0.0 else v
    p = 10 ** dec
    sc = int(a * float(p) + 0.5)
    s = ("-" if (v < 0.0 and sc != 0) else "") + str(sc // p)
    if dec > 0:
        s += "." + str(sc % p).zfill(dec)
    return s


def debug_lines(d):
    fps, x, y, z, yaw, pitch, hb, bx, by, bz, bid, nx, ny, nz = d
    lines = ["FPS: %d" % fps, "XYZ: %s %s %s" % (fixed(x, 3), fixed(y, 3), fixed(z, 3))]
    lines.append("Block: %d %d %d (id %d)" % (bx, by, bz, bid) if hb else "Block: none")
    if not hb:
        face = "-"
    elif nx == 0 and ny == 0 and nz == 0:
        face = "inside"
    elif nx != 0:
        face = ("+" if nx > 0 else "-") + "x"
    elif ny != 0:
        face = ("+" if ny > 0 else "-") + "y"
    else:
        face = ("+" if nz > 0 else "-") + "z"
    lines.append("Face: " + face)
    lines.append("Yaw: %s Pitch: %s" % (fixed(yaw, 1), fixed(pitch, 1)))
    return lines


def f64(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]


def parse_dbg(t):
    fps = int(t[0])
    x, y, z, yaw, pitch = (f64(v) for v in t[1:6])
    return (fps, x, y, z, yaw, pitch, int(t[6]) == 1, int(t[7]), int(t[8]), int(t[9]), int(t[10]), int(t[11]), int(t[12]), int(t[13]))


def scale_for(w, h):
    return max(1, min(4, w // 320, h // 240)) if False else max(1, min(min(w // 320, h // 240), 4))


def main():
    d = sys.argv[1]
    bad = 0
    img = None
    name = None
    for line in open(os.path.join(d, "scenes.txt")):
        t = line.split()
        if not t:
            continue
        op, a = t[0], t[1:]
        if op == "scene":
            name, img = a[0], Img(int(a[1]), int(a[2]))
        elif op == "rect":
            img.rect(int(a[0]), int(a[1]), int(a[2]), int(a[3]), int(a[4], 16))
        elif op == "invert":
            img.invert(*(int(v) for v in a[:4]))
        elif op == "glyph":
            img.glyph(int(a[0]), int(a[1]), int(a[2]), int(a[3], 16), int(a[4]))
        elif op == "text":
            data = bytes(int(a[5][i:i + 3], 16) - 256 for i in range(0, len(a[5]), 3)) if len(a) > 5 else b""
            img.text(int(a[0]), int(a[1]), int(a[2]), int(a[3], 16), int(a[4], 16), data)
        elif op == "cross":
            img.crosshair(int(a[0]))
        elif op == "hotbar":
            img.hotbar(int(a[0]), int(a[1]))
        elif op == "debug":
            img.debug(int(a[0]), parse_dbg(a[1:]))
        elif op == "frame":
            s, flags, slot = int(a[0]), int(a[1]), int(a[2])
            if flags & 1:
                img.crosshair(s)
            if flags & 2:
                img.hotbar(s, slot)
            if flags & 4:
                img.debug(s, parse_dbg(a[3:]))
        elif op == "hash":
            mine = img.bytes()
            theirs = open(os.path.join(d, name + ".rgba"), "rb").read()
            hx = hashlib.sha256(mine).hexdigest()
            if mine != theirs:
                n = sum(1 for i in range(0, len(mine), 4) if mine[i:i + 4] != theirs[i:i + 4])
                print("  DIFF  %s: %d pixels differ" % (name, n))
                bad += 1
            elif hx != a[0]:
                print("  DIFF  %s: hash %s != %s" % (name, hx[:12], a[0][:12]))
                bad += 1
            else:
                print("  ok    %s (%dx%d) %s" % (name, img.w, img.h, hx[:12]))
        else:
            print("  DIFF  unknown op", op)
            bad += 1
    # invariants: fixed point formatting and the debug text of known values
    chk = [(fixed(12.3456, 3), "12.346"), (fixed(-3.5, 3), "-3.500"), (fixed(-0.0004, 3), "0.000"), (fixed(99.95, 1), "100.0"),
           (fixed(2.5, 0), "3"), (fixed(1e12, 2), "?")]
    for got, want in chk:
        if got != want:
            print("  DIFF  format %r != %r" % (got, want))
            bad += 1
    print("  %d problems" % bad)
    sys.exit(1 if bad else 0)


main()
