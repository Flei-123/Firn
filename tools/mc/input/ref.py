#!/usr/bin/env python3
"""tools/mc/input/ref.py -- independent reference for lib/mc/mc_input.fi (written from docs/mc/input.md, not from the Firn code).

Reads <dir>/scenes.txt, worldA.bin, worldB.bin and the box framebuffers written by dump.fi and checks
  * mouse look: yaw / pitch BIT identical (no trig involved),
  * movement physics: every tick of 7 scripted scenarios re-simulated with python floats (positions within 1e-9, flags exact);
    collision = exact swept AABB against unit cubes written from the documented rules (y, x, z order, 1e-9 penetration rule,
    unloaded chunks solid),
  * picking: 600 random rays decided by a brute force slab test against every pickable block near the ray (not a DDA):
    same block, same face normal, distance within 1e-9, same hit / miss,
  * selection box lines: the three framebuffers pixel for pixel (own integer line stepper) + SHA-256.
Usage: ref.py <dir>   (prints one line per part, exit 1 on any difference)
"""
import hashlib
import math
import struct
import sys

OFF = 1000000


def f64(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def load_world(path):
    data = open(path, "rb").read()
    ids = struct.unpack("<%dH" % (32 * 32 * 32), data)

    def block(x, y, z):  # None = chunk not loaded, 0.. = id, -2 = outside stored height
        if x < 0 or x >= 32 or z < 0 or z >= 32:
            return None
        ly = y + 64
        if ly < 0 or ly >= 32:
            return -2
        return ids[(ly * 32 + z) * 32 + x]
    return block


# ---------------------------------------------------------------- look
def wrap_yaw(y):
    y = y - 360.0 * math.floor((y + 180.0) / 360.0)
    if y >= 180.0:
        y -= 360.0
    return y


def look(yaw, pitch, sens, dx, dy):
    f = sens * 0.6 + 0.2
    k = (f * f * f * 8.0) * 0.15
    yaw = wrap_yaw(yaw + dx * k)
    pitch = max(-90.0, min(90.0, pitch + dy * k))
    return yaw, pitch


# ---------------------------------------------------------------- physics
SOLID_IDS = {1}


def is_solid(block, x, y, z):
    b = block(x, y, z)
    return b is None or b in SOLID_IDS


def move_axis(block, box, axis, d):
    """distance moved along `axis` (0 x, 1 y, 2 z) of the box [x0,y0,z0,x1,y1,z1] against solid unit cubes"""
    lo, hi = box[axis], box[axis + 3]
    others = [a for a in range(3) if a != axis]
    rng = []
    for a in range(3):
        a0 = box[a] + (min(d, 0.0) if a == axis else 0.0)
        a1 = box[a + 3] + (max(d, 0.0) if a == axis else 0.0)
        rng.append(range(math.floor(a0) - 1, math.floor(a1) + 2))
    out = d
    for cx in rng[0]:
        for cy in rng[1]:
            for cz in rng[2]:
                if not is_solid(block, cx, cy, cz):
                    continue
                c = (cx, cy, cz)
                ok = True
                for o in others:
                    if not (box[o] + 1e-9 < c[o] + 1.0 and c[o] + 1e-9 < box[o + 3]):
                        ok = False
                if not ok:
                    continue
                if out > 0.0 and hi <= c[axis]:
                    out = min(out, c[axis] - hi)
                elif out < 0.0 and lo >= c[axis] + 1.0:
                    out = max(out, c[axis] + 1.0 - lo)
    return out


def simulate(block, start, yaw, ticks):
    x, y, z = start
    pitch = 0.0
    vy = 0.0
    ground = flying = noclip = False
    slot = 0
    sens = 0.5
    res = []
    for t in ticks:
        dxm, dym, keys, tf, tn, sd = t
        if tf:
            flying = not flying
            vy = 0.0
        if tn:
            noclip = not noclip
        if sd:
            slot = (slot + sd) % 9
        yaw, pitch = look(yaw, pitch, sens, dxm, dym)
        fwd = bool(keys & 1)
        sneak = bool(keys & 32)
        sprint = bool(keys & 64) and fwd and not sneak
        mx = (1.0 if keys & 8 else 0.0) - (1.0 if keys & 4 else 0.0)
        mz = (1.0 if fwd else 0.0) - (1.0 if keys & 2 else 0.0)
        yr = yaw * (math.pi / 180.0)
        sn, cs = math.sin(yr), math.cos(yr)
        wx = (0.0 - cs) * mx + (0.0 - sn) * mz
        wz = (0.0 - sn) * mx + cs * mz
        if mx != 0.0 and mz != 0.0:
            wx *= 0.7071067811865476
            wz *= 0.7071067811865476
        if flying:
            speed = 0.546 * (2.0 if sprint else 1.0)
        elif sneak:
            speed = 0.0648
        elif sprint:
            speed = 0.2806
        else:
            speed = 0.21585
        dx, dz = wx * speed, wz * speed
        dy = 0.0
        if flying:
            if keys & 16:
                dy += 0.375
            if keys & 32:
                dy -= 0.375
        else:
            if (keys & 16) and ground:
                vy = 0.42
            dy = vy
        if noclip:
            x, y, z = x + dx, y + dy, z + dz
            ground = False
            if not flying:
                vy = (vy - 0.08) * 0.98
        else:
            h = 1.5 if sneak else 1.8
            box = [x - 0.3, y, z - 0.3, x + 0.3, y + h, z + 0.3]
            my = move_axis(block, box, 1, dy)
            box[1] += my
            box[4] += my
            mxx = move_axis(block, box, 0, dx)
            box[0] += mxx
            box[3] += mxx
            mzz = move_axis(block, box, 2, dz)
            x, y, z = x + mxx, y + my, z + mzz
            hit_y = my != dy
            ground = hit_y and dy < 0.0
            if flying:
                vy = 0.0
            else:
                if hit_y:
                    vy = 0.0
                vy = (vy - 0.08) * 0.98
        res.append((x, y, z, yaw, pitch, ground, slot))
    return res


# ---------------------------------------------------------------- picking (slab test, brute force)
def pick(block, o, d, reach):
    ln = math.sqrt((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2])
    d = [d[0] / ln, d[1] / ln, d[2] / ln]

    def pickable(x, y, z):
        b = block(x, y, z)
        return b is not None and b in (1, 2)

    cell = [math.floor(o[i]) for i in range(3)]
    if pickable(*cell):
        return (cell, (0, 0, 0), 0.0)
    best = None
    r = int(math.ceil(reach)) + 1
    for cx in range(cell[0] - r, cell[0] + r + 1):
        for cy in range(cell[1] - r, cell[1] + r + 1):
            for cz in range(cell[2] - r, cell[2] + r + 1):
                if not pickable(cx, cy, cz):
                    continue
                c = (cx, cy, cz)
                tmin, tmax, axis = -math.inf, math.inf, -1
                ok = True
                for a in range(3):
                    if d[a] == 0.0:
                        if o[a] < c[a] or o[a] >= c[a] + 1:
                            ok = False
                            break
                        continue
                    t0 = (c[a] - o[a]) / d[a]
                    t1 = (c[a] + 1 - o[a]) / d[a]
                    if t0 > t1:
                        t0, t1 = t1, t0
                    if t0 > tmin:
                        tmin, axis = t0, a
                    tmax = min(tmax, t1)
                if not ok or tmin > tmax or tmin < 0.0 or tmin > reach:
                    continue
                if best is None or tmin < best[2]:
                    n = [0, 0, 0]
                    n[axis] = -1 if d[axis] > 0 else 1
                    best = (list(c), tuple(n), tmin)
    return best


# ---------------------------------------------------------------- lines
def floor_div(a, b):
    return a // b


def draw_box(fb, w, h, depth, cam, box, color):
    pos, right, up, fwd, near, tan_x, tan_y = cam
    hw, hh = w * 0.5, h * 0.5
    fpx, fpy = hw / tan_x, hh / tan_y
    e = 0.002
    x0, y0, z0 = box[0] - e, box[1] - e, box[2] - e
    x1, y1, z1 = box[0] + 1.0 + e, box[1] + 1.0 + e, box[2] + 1.0 + e
    P = lambda a, b, c: (a, b, c)
    edges = [
        (P(x0, y0, z0), P(x1, y0, z0)), (P(x1, y0, z0), P(x1, y0, z1)), (P(x1, y0, z1), P(x0, y0, z1)), (P(x0, y0, z1), P(x0, y0, z0)),
        (P(x0, y1, z0), P(x1, y1, z0)), (P(x1, y1, z0), P(x1, y1, z1)), (P(x1, y1, z1), P(x0, y1, z1)), (P(x0, y1, z1), P(x0, y1, z0)),
        (P(x0, y0, z0), P(x0, y1, z0)), (P(x1, y0, z0), P(x1, y1, z0)), (P(x1, y0, z1), P(x1, y1, z1)), (P(x0, y0, z1), P(x0, y1, z1)),
    ]

    def view(p):
        rel = [p[i] - pos[i] for i in range(3)]
        return [(rel[0] * v[0] + rel[1] * v[1]) + rel[2] * v[2] for v in (right, up, fwd)]

    for a, b in edges:
        va, vb = view(a), view(b)
        if va[2] < near and vb[2] < near:
            continue
        if va[2] < near:
            t = (near - va[2]) / (vb[2] - va[2])
            va = [va[0] + (vb[0] - va[0]) * t, va[1] + (vb[1] - va[1]) * t, near]
        elif vb[2] < near:
            t = (near - vb[2]) / (va[2] - vb[2])
            vb = [vb[0] + (va[0] - vb[0]) * t, vb[1] + (va[1] - vb[1]) * t, near]
        iz0, iz1 = 1.0 / va[2], 1.0 / vb[2]
        s = [hw + (va[0] * iz0) * fpx, hh - (va[1] * iz0) * fpy, hw + (vb[0] * iz1) * fpx, hh - (vb[1] * iz1) * fpy]
        if not all(abs(v) < 100000.0 for v in s):
            continue
        q = [math.floor(v * 16.0 + 0.5) // 16 for v in s]
        ddx, ddy = q[2] - q[0], q[3] - q[1]
        n = max(abs(ddx), abs(ddy))
        for i in range(n + 1):
            if n > 0:
                px = q[0] + (2 * ddx * i + n) // (2 * n)
                py = q[1] + (2 * ddy * i + n) // (2 * n)
                t = i / n
            else:
                px, py, t = q[0], q[1], 0.0
            if 0 <= px < w and 0 <= py < h:
                iz = iz0 + (iz1 - iz0) * t
                idx = py * w + px
                if iz * 1.002 > depth[idx]:
                    fb[idx * 4:idx * 4 + 4] = color


def main(d):
    lines = [l.split() for l in open(d + "/scenes.txt") if l.strip()]
    bad = 0
    i = 0
    wa, wb = load_world(d + "/worldA.bin"), load_world(d + "/worldB.bin")
    nl = nph = nray = nhit = nbox = 0
    while i < len(lines):
        t = lines[i]
        if t[0] == "look":
            sens = f64(t[1])
            yaw = pitch = 0.0
            i += 1
            while i < len(lines) and lines[i][0] == "l":
                dx, dy = f64(lines[i][1]), f64(lines[i][2])
                yaw, pitch = look(yaw, pitch, sens, dx, dy)
                if bits(yaw) != int(lines[i][3], 16) or bits(pitch) != int(lines[i][4], 16):
                    bad += 1
                    print("  DIFF  look sens %g step: %r vs %r" % (sens, (yaw, pitch), (f64(lines[i][3]), f64(lines[i][4]))))
                    break
                i += 1
            while i < len(lines) and lines[i][0] == "l":
                i += 1
            nl += 1
            continue
        if t[0] == "phys":
            name, start, yaw0, n = t[1], [f64(x) for x in t[2:5]], f64(t[5]), int(t[6])
            rows = lines[i + 1:i + 1 + n]
            i += 1 + n
            ticks = [(f64(r[1]), f64(r[2]), int(r[3]), int(r[4]), int(r[5]), int(r[6]) - OFF) for r in rows]
            sim = simulate(wa, start, yaw0, ticks)
            worst = 0.0
            msg = None
            for k, (r, s) in enumerate(zip(rows, sim)):
                fx = [f64(r[7]), f64(r[8]), f64(r[9])]
                err = max(abs(fx[0] - s[0]), abs(fx[1] - s[1]), abs(fx[2] - s[2]))
                worst = max(worst, err)
                if err > 1e-9 or bits(s[3]) != int(r[10], 16) or bits(s[4]) != int(r[11], 16) or int(r[12]) != int(s[5]) or int(r[13]) - OFF != s[6]:
                    msg = "tick %d: firn %s ground=%s slot=%d, python %s" % (k, fx, r[12], int(r[13]) - OFF, s)
                    break
            if msg:
                bad += 1
                print("  DIFF  phys %s: %s" % (name, msg))
            else:
                last = sim[-1]
                print("  ok    phys %-12s %2d ticks, end (%.4f, %.4f, %.4f) ground=%s, max error %.1e" % (name, n, last[0], last[1], last[2], last[5], worst))
            nph += 1
            continue
        if t[0] == "tables":
            i += 1
            continue
        if t[0] == "picks":
            i += 1
            continue
        if t[0] == "ray":
            o = [f64(x) for x in t[1:4]]
            dd = [f64(x) for x in t[4:7]]
            reach = f64(t[7])
            hit = t[8] == "1"
            ref = pick(wb, o, dd, reach)
            nray += 1
            if hit:
                nhit += 1
            if (ref is not None) != hit:
                bad += 1
                print("  DIFF  ray %d: firn hit=%s python=%s (o=%s d=%s)" % (nray, hit, ref, o, dd))
            elif hit:
                cell = [int(x) - OFF for x in t[9:12]]
                nrm = tuple(int(x) - OFF for x in t[12:15])
                dist = f64(t[15])
                if cell != ref[0] or nrm != ref[1] or abs(dist - ref[2]) > 1e-9:
                    bad += 1
                    print("  DIFF  ray %d: firn %s %s %g, python %s" % (nray, cell, nrm, dist, ref))
            i += 1
            continue
        if t[0] == "box":
            name, w, h, occ = t[1], int(t[2]), int(t[3]), t[4] == "1"
            box = [int(x) - OFF for x in t[5:8]]
            c = lines[i + 1]
            v = [f64(x) for x in c[1:]]
            cam = (v[0:3], v[3:6], v[6:9], v[9:12], v[12], v[13], v[14])
            hs = lines[i + 2][1]
            i += 3
            fb = bytearray(b"\x30\x30\x30\xff" * (w * h))
            depth = [0.0] * (w * h)
            if occ:
                for y in range(h):
                    for x in range(w // 2):
                        depth[y * w + x] = 0.25
            draw_box(fb, w, h, depth, cam, box, b"\x00\x00\x00\xff")
            draw_box(fb, w, h, depth, cam, [box[0] + 2, box[1], box[2] + 1], b"\xff\xff\xff\xff")
            fire = open("%s/%s.rgba" % (d, name), "rb").read()
            dark = sum(1 for k in range(0, len(fb), 4) if fb[k:k + 4] == b"\x00\x00\x00\xff")
            if bytes(fb) != fire:
                cnt = sum(1 for k in range(0, len(fb), 4) if fb[k:k + 4] != fire[k * 1:k * 1 + 4])
                bad += 1
                print("  DIFF  box %s: framebuffer differs (%d px)" % (name, cnt))
            elif hashlib.sha256(bytes(fb)).hexdigest() != hs:
                bad += 1
                print("  DIFF  box %s: hash" % name)
            else:
                print("  ok    box %-13s %d line pixels (black), hash %s" % (name, dark, hs[:12]))
            nbox += 1
            continue
        i += 1
    print("  ok    look: %d sequences bit identical; %d rays (%d hits) vs brute force slab test" % (nl, nray, nhit) if bad == 0 else "  looks/rays: see DIFF above")
    if nl != 3 or nph != 7 or nray != 600 or nbox != 4 or nhit < 150:
        bad += 1
        print("  DIFF  scene counts look=%d phys=%d rays=%d (hits %d) box=%d" % (nl, nph, nray, nhit, nbox))
    print("  %d problems" % bad)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
