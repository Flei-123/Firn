#!/usr/bin/env python3
"""tools/mc/raster/ref.py -- independent reference rasterizer for lib/mc/mc_raster.fi.

Reads <dir>/scenes.txt + <dir>/atlas.bin (written by dump.fi), renders every scene on its own and compares
the result with the Firn framebuffer <dir>/<scene>.rgba byte for byte and with the SHA-256 in scenes.txt.

Written from the spec in docs/mc/raster.md, not from the Firn code:
  * floats are IEEE f64 in the same operation order as the spec (python has no fused multiply add);
  * coverage is decided with exact python integers: every pixel evaluates the three edge functions
    directly (no incremental stepping), top-left fill rule as in the spec;
  * texture, shading and fog are integer arithmetic.
Usage: ref.py <dir> [--skip-compare]      (prints one line per scene, exit 1 on any difference)
"""
import hashlib
import math
import struct
import sys


def f64(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]


def parse(path):
    scenes = []
    cur = None
    mesh = None
    for line in open(path):
        t = line.split()
        if not t:
            continue
        if t[0] == "scene":
            cur = dict(name=t[1], w=int(t[2]), h=int(t[3]), cull=t[4] == "1", fog=t[5] == "1",
                       fs=f64(t[6]), fe=f64(t[7]), clear=int(t[8], 16), meshes=[], hash=None,
                       light=(256, 256, 224, 176))
            scenes.append(cur)
        elif t[0] == "cam":
            v = [f64(x) for x in t[1:]]
            cur["cam"] = dict(pos=v[0:3], right=v[3:6], up=v[6:9], fwd=v[9:12], near=v[12], tan_x=v[13], tan_y=v[14])
        elif t[0] == "light":
            cur["light"] = tuple(int(x) for x in t[1:5])    # day, tint r, g, b
        elif t[0] == "mesh":
            mesh = dict(o=[int(t[1]) - 1000000, int(t[2]) - 1000000, int(t[3]) - 1000000], verts=[], idx=[],
                        trans=len(t) > 6 and t[6] == "1")
            cur["meshes"].append(mesh)
        elif t[0] == "box":
            v = [f64(x) for x in t[1:9]]
            cur["meshes"].append(dict(box=v, rgb=(int(t[9]), int(t[10]), int(t[11]))))
        elif t[0] == "v":
            mesh["verts"].append((int(t[1], 16), int(t[2], 16)))
        elif t[0] == "i":
            mesh["idx"] = [int(x) for x in t[1:]]
        elif t[0] == "hash":
            cur["hash"] = t[1]
    return scenes


FACE_K = [204, 204, 128, 256, 153, 153]
LIGHT_MIN = 24


def light_factor(s256, b256, tint):
    lum = max(s256, (b256 * tint) // 256)
    return LIGHT_MIN + (lum * (256 - LIGHT_MIN)) // 256


class Scene:
    def __init__(self, sc, atlas):
        self.sc = sc
        self.atlas = atlas
        self.w, self.h = sc["w"], sc["h"]
        c = sc["cam"]
        self.cam = c
        self.hw = self.w * 0.5
        self.hh = self.h * 0.5
        self.fpx = self.hw / c["tan_x"]
        self.fpy = self.hh / c["tan_y"]
        clear = sc["clear"]
        self.color = [clear] * (self.w * self.h)
        self.depth = [0.0] * (self.w * self.h)
        self.trans = False
        self.flat = None

    # ---- view space
    def to_view(self, wx, wy, wz):
        c = self.cam
        rx = wx - c["pos"][0]
        ry = wy - c["pos"][1]
        rz = wz - c["pos"][2]
        r, u, f = c["right"], c["up"], c["fwd"]
        return (rx * r[0] + ry * r[1] + rz * r[2],
                rx * u[0] + ry * u[1] + rz * u[2],
                rx * f[0] + ry * f[1] + rz * f[2])

    def draw_box(self, item):
        x, y, z, sx, sy, sz, ys, yc = item["box"]
        r, g, b = item["rgb"]
        self.flat = (r & 255) | ((g & 255) << 8) | ((b & 255) << 16)
        half = [sx * 0.5, sy * 0.5, sz * 0.5]
        for f in range(6):
            axis = f // 2
            ua = (axis + 1) % 3
            va = (axis + 2) % 3
            sgn = 1.0 if f % 2 == 1 else -1.0
            vs = []
            for k in range(4):
                su = 1.0 if k in (1, 2) else -1.0
                sv = 1.0 if k in (2, 3) else -1.0
                q = [0.0, 0.0, 0.0]
                q[axis] = sgn * half[axis]
                q[ua] = su * half[ua]
                q[va] = sv * half[va]
                wx = x + (q[0] * yc + q[2] * ys)
                wy = y + q[1]
                wz = z + (0.0 - q[0] * ys + q[2] * yc)
                vx, vy, vz = self.to_view(wx, wy, wz)
                vs.append([vx, vy, vz, 0.0, 0.0, 3.0, 15.0, 0.0])
            fk = FACE_K[f]
            if f % 2 == 1:
                self.triangle([vs[0][:], vs[1][:], vs[2][:]], 1, fk)
                self.triangle([vs[0][:], vs[2][:], vs[3][:]], 1, fk)
            else:
                self.triangle([vs[0][:], vs[2][:], vs[1][:]], 1, fk)
                self.triangle([vs[0][:], vs[3][:], vs[2][:]], 1, fk)
        self.flat = None

    def draw_mesh(self, mesh):
        if "box" in mesh:
            self.draw_box(mesh)
            return
        self.trans = mesh.get("trans", False)
        try:
            self.draw_mesh_inner(mesh)
        finally:
            self.trans = False

    def draw_mesh_inner(self, mesh):
        ox, oy, oz = mesh["o"]
        vs = []
        for w0, w1 in mesh["verts"]:
            x = float(ox + (w0 & 255))
            y = float(oy + ((w0 >> 8) & 255))
            z = float(oz + ((w0 >> 16) & 255))
            vx, vy, vz = self.to_view(x, y, z)
            vs.append([vx, vy, vz, float((w1 >> 14) & 31), float((w1 >> 19) & 31), float((w0 >> 27) & 3),
                       float((w1 >> 24) & 15), float((w1 >> 28) & 15)])
        idx = mesh["idx"]
        n = len(mesh["verts"])
        for t in range(0, len(idx) - 2, 3):
            a, b, c = idx[t], idx[t + 1], idx[t + 2]
            if a < n and b < n and c < n:
                w0, w1 = mesh["verts"][a]
                self.triangle([vs[a][:], vs[b][:], vs[c][:]], w1 & 16383, FACE_K[(w0 >> 24) & 7])

    # ---- near clipping, Sutherland-Hodgman against z >= near
    def triangle(self, tri, tile, fk):
        if tile == 0:
            return
        near = self.cam["near"]
        poly = []
        for i in range(3):
            p, q = tri[i], tri[(i + 1) % 3]
            pin, qin = p[2] >= near, q[2] >= near
            if pin:
                poly.append(p)
            if pin != qin:
                s = (near - p[2]) / (q[2] - p[2])
                o = [p[k] + (q[k] - p[k]) * s for k in range(8)]
                o[2] = near
                poly.append(o)
        if len(poly) >= 3:
            self.fill(poly[0], poly[1], poly[2], tile, fk)
        if len(poly) >= 4:
            self.fill(poly[0], poly[2], poly[3], tile, fk)

    def fill(self, A, B, C, tile, fk):
        pts = []
        for v in (A, B, C):
            iz = 1.0 / v[2]
            sx = self.hw + (v[0] * iz) * self.fpx
            sy = self.hh - (v[1] * iz) * self.fpy
            if not (abs(sx) < 1e6 and abs(sy) < 1e6):
                return
            pts.append((math.floor(sx * 16.0 + 0.5), math.floor(sy * 16.0 + 0.5), iz, v))
        (ax, ay, za, va), (bx, by, zb, vb), (cx, cy, zc, vc) = pts
        area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        if area == 0:
            return
        if area > 0 and self.sc["cull"]:
            return
        P = [(ax, ay, za, va), (bx, by, zb, vb), (cx, cy, zc, vc)]
        if area < 0:                    # make the winding positive: swap the last two vertices
            P = [P[0], P[2], P[1]]
            area = -area
        (ax, ay, za, va), (bx, by, zb, vb), (cx, cy, zc, vc) = P
        # perspective-premultiplied attributes
        ua, ub, uc = va[3] * za, vb[3] * zb, vc[3] * zc
        vva, vvb, vvc = va[4] * za, vb[4] * zb, vc[4] * zc
        oa, ob, oc = va[5] * za, vb[5] * zb, vc[5] * zc
        ka, kb, kc = va[6] * za, vb[6] * zb, vc[6] * zc
        la, lb, lc = va[7] * za, vb[7] * zb, vc[7] * zc
        day, tint_r, tint_g, tint_b = self.sc.get("light", (256, 256, 224, 176))
        dayf = float(day)
        # edges (p -> q) opposite to vertex 0, 1, 2
        edges = [((bx, by), (cx, cy)), ((cx, cy), (ax, ay)), ((ax, ay), (bx, by))]

        def inc(e):
            (px, py), (qx, qy) = e
            dx, dy = qx - px, qy - py
            return dy < 0 or (dy == 0 and dx > 0)    # top-left edge: pixels exactly on it belong to the triangle

        tl = [inc(e) for e in edges]
        x0 = max(0, min(ax, bx, cx) // 16)
        x1 = min(self.w - 1, max(ax, bx, cx) // 16)
        y0 = max(0, min(ay, by, cy) // 16)
        y1 = min(self.h - 1, max(ay, by, cy) // 16)
        areaf = float(area)
        T = self.atlas["tile"]
        fog = self.sc["fog"]
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                px, py = x * 16 + 8, y * 16 + 8
                E = []
                ok = True
                for k in range(3):
                    (qx, qy), (rx, ry) = edges[k]
                    e = (rx - qx) * (py - qy) - (ry - qy) * (px - qx)
                    if e < 0 or (e == 0 and not tl[k]):
                        ok = False
                        break
                    E.append(e)
                if not ok:
                    continue
                f0, f1, f2 = float(E[0]), float(E[1]), float(E[2])
                s = f0 * za + f1 * zb + f2 * zc
                zinv = s / areaf
                i = y * self.w + x
                if not zinv > self.depth[i]:
                    continue
                u = (f0 * ua + f1 * ub + f2 * uc) / s
                v = (f0 * vva + f1 * vvb + f2 * vvc) / s
                ao = (f0 * oa + f1 * ob + f2 * oc) / s
                sk = (f0 * ka + f1 * kb + f2 * kc) / s
                bk = (f0 * la + f1 * lb + f2 * lc) / s
                tx = int(math.floor(u * T)) & (T - 1)
                ty = int(math.floor(v * T)) & (T - 1)
                if self.flat is not None:
                    tp = self.flat | 0xFF000000
                else:
                    tp = self.texel(tile, tx, ty)
                alpha = tp >> 24
                if alpha < (1 if self.trans else 128):
                    continue
                fq = min(256, max(0, int(math.floor(100.0 + 52.0 * ao))))
                total = (fq * fk) // 256
                s256 = min(256, max(0, int(math.floor(sk * dayf / 15.0 + 0.5))))
                b256 = min(256, max(0, int(math.floor(bk * 256.0 / 15.0 + 0.5))))
                tr = (total * light_factor(s256, b256, tint_r)) // 256
                tg = (total * light_factor(s256, b256, tint_g)) // 256
                tb = (total * light_factor(s256, b256, tint_b)) // 256
                r = ((tp & 255) * tr) // 256
                g = (((tp >> 8) & 255) * tg) // 256
                b = (((tp >> 16) & 255) * tb) // 256
                if fog:
                    z = 1.0 / zinv
                    t = (z - self.sc["fs"]) / (self.sc["fe"] - self.sc["fs"])
                    t = min(1.0, max(0.0, t))
                    fg = int(math.floor(t * 256.0))
                    fr, fgc, fb = self.sc.get("fogc", (150, 190, 230))
                    r = (r * (256 - fg) + fr * fg) // 256
                    g = (g * (256 - fg) + fgc * fg) // 256
                    b = (b * (256 - fg) + fb * fg) // 256
                if self.trans:
                    d = self.color[i]
                    r = (r * alpha + (d & 255) * (255 - alpha) + 127) // 255
                    g = (g * alpha + ((d >> 8) & 255) * (255 - alpha) + 127) // 255
                    b = (b * alpha + ((d >> 16) & 255) * (255 - alpha) + 127) // 255
                else:
                    self.depth[i] = zinv
                self.color[i] = r | (g << 8) | (b << 16) | 0xFF000000

    def texel(self, tile, tx, ty):
        a = self.atlas
        col = (tile - 1) % a["cols"]
        row = (tile - 1) // a["cols"]
        o = ((row * a["tile"] + ty) * a["w"] + col * a["tile"] + tx) * 4
        d = a["data"]
        return d[o] | (d[o + 1] << 8) | (d[o + 2] << 16) | (d[o + 3] << 24)

    def rgba(self):
        return struct.pack("<%dI" % len(self.color), *self.color)


def read_png(path):
    """minimal PNG decoder: 8 bit RGBA, all five filters -> (w, h, bytes)"""
    import zlib
    d = open(path, "rb").read()
    assert d[:8] == b"\x89PNG\r\n\x1a\n"
    pos, idat, w, h = 8, b"", 0, 0
    while pos < len(d):
        n = struct.unpack(">I", d[pos:pos + 4])[0]
        kind = d[pos + 4:pos + 8]
        body = d[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and ctype == 6
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    st = w * 4
    out = bytearray()
    prev = bytearray(st)
    for y in range(h):
        ft = raw[y * (st + 1)]
        line = bytearray(raw[y * (st + 1) + 1:(y + 1) * (st + 1)])
        for i in range(st):
            a = line[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            if ft == 1:
                line[i] = (line[i] + a) & 255
            elif ft == 2:
                line[i] = (line[i] + b) & 255
            elif ft == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 255
        out += line
        prev = line
    return w, h, bytes(out)


def read_ppm(path):
    d = open(path, "rb").read()
    parts = d.split(b"\n", 3)
    assert parts[0] == b"P6" and parts[2] == b"255"
    w, h = [int(x) for x in parts[1].split()]
    return w, h, parts[3]


def main():
    d = sys.argv[1]
    atlas = dict(tile=16, cols=16, w=256, data=open(d + "/atlas.bin", "rb").read())
    scenes = parse(d + "/scenes.txt")
    bad = 0
    for sc in scenes:
        s = Scene(sc, atlas)
        for m in sc["meshes"]:
            s.draw_mesh(m)
        img = s.rgba()
        hx = hashlib.sha256(img).hexdigest()
        if "--skip-compare" in sys.argv:
            print("scene %s hash %s" % (sc["name"], hx))
            continue
        fb = open("%s/%s.rgba" % (d, sc["name"]), "rb").read()
        diff = sum(1 for i in range(0, len(fb), 4) if fb[i:i + 4] != img[i:i + 4]) if len(fb) == len(img) else -1
        painted = sum(1 for c in s.color if c != sc["clear"])
        pw, ph, pd = read_png("%s/%s.png" % (d, sc["name"]))
        qw, qh, qd = read_ppm("%s/%s.ppm" % (d, sc["name"]))
        files_ok = (pw, ph) == (sc["w"], sc["h"]) and pd == fb and (qw, qh) == (sc["w"], sc["h"]) and \
            qd == b"".join(fb[i:i + 3] for i in range(0, len(fb), 4))
        status = "ok" if (diff == 0 and hx == sc["hash"] and files_ok) else "DIFF"
        if not files_ok:
            print("  DIFF  %s: PNG/PPM file does not match the framebuffer" % sc["name"])
        if status != "ok":
            bad += 1
        print("  %-5s %-14s %4dx%-4d painted %6d px, differing pixels %d, hash %s" % (
            status, sc["name"], sc["w"], sc["h"], painted, diff, hx[:16]))
    if "--skip-compare" not in sys.argv:
        names = {s["name"]: s["hash"] for s in scenes}
        if names.get("depth_ab") != names.get("depth_ba"):
            print("  DIFF  depth_ab and depth_ba must be identical")
            bad += 1
        if names.get("inside_cull") == names.get("inside_nocull"):
            print("  DIFF  culling has no effect from inside a block")
            bad += 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
