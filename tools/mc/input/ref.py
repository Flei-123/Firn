#!/usr/bin/env python3
"""tools/mc/input/ref.py -- independent reference for lib/mc/mc_input.fi (written from docs/mc/input.md, not from the Firn code).

Reads <dir>/scenes.txt, worldA.bin, worldB.bin and the box framebuffers written by dump.fi and checks
  * mouse look: yaw / pitch BIT identical (no trig involved),
  * movement physics: every tick of 19 scripted scenarios re-simulated with the vanilla momentum model in python (f32 constants, positions within 1e-9, flags exact);
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


# ---------------------------------------------------------------- physics (vanilla momentum model, docs/mc/input.md)
def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


SIN_SCALE = f32(10430.378)
DEG = f32(0.017453292)
BOXES_A = {1: [(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)]}
BOXES_C = {
    1: [(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)],
    4: [(0.0, 0.0, 0.0, 1.0, 0.875, 1.0)],
    5: [(0.0625, 0.0, 0.0625, 0.9375, 0.9375, 0.9375)],
    8: [(0.4375, 0.0, 0.4375, 0.5625, 1.0, 0.5625)],
    9: [(0.4375, 0.0, 0.4375, 0.5625, 1.0, 0.5625)],
}
OFFSET_MAX = {8: 0.25, 9: 0.125}
KIND = {4: "sand", 5: "honey", 6: "web", 7: "berry"}
STUCK_WEB_Y = 0.05000000074505806
STEP_HEIGHT = 0.6
BELOW = 0.500001
DRAG_Y = float(f32(0.98))
BASE_DRAG = float(f32(0.91))
SPEED_FACTOR = float(f32(0.4))
GRAVITY = 0.08


def s64(v):
    v &= (1 << 64) - 1
    return v - (1 << 64) if v >> 63 else v


def block_seed(x, y, z):
    xm = (x * 3129871) & 0xFFFFFFFF
    if xm >> 31:
        xm -= 1 << 32
    l = s64(xm ^ s64(z * 116129781) ^ y)
    l = s64(s64(s64(l * l) * 42317861) + s64(l * 11))
    return l >> 16


def block_offset(x, z, mx):
    sd = block_seed(x, 0, z)
    a = f32(f32(sd & 15) / f32(15.0))
    b = f32(f32((sd >> 8) & 15) / f32(15.0))
    return [max(-mx, min(mx, (v - 0.5) * 0.5)) for v in (a, b)]


def mth_sin(x):
    return f32(math.sin((int(f32(x * SIN_SCALE)) & 65535) * math.pi * 2.0 / 65536.0))


def mth_cos(x):
    return f32(math.sin((int(f32(f32(x * SIN_SCALE) + f32(16384.0))) & 65535) * math.pi * 2.0 / 65536.0))


def yaw_rad(yaw):
    return f32(f32(yaw) * DEG)


def move_relative(v, strafe, forward, speed, yaw):
    x, z = float(strafe), float(forward)
    d0 = x * x + z * z
    if d0 < 0.0000001:
        return
    ix, iz = x, z
    if d0 > 1.0:
        d = math.sqrt(d0)
        if d < 0.00001:
            ix = iz = 0.0
        else:
            ix, iz = x / d, z / d
    ax, az = ix * float(speed), iz * float(speed)
    r = yaw_rad(yaw)
    sn, cs = float(mth_sin(r)), float(mth_cos(r))
    v[0] += ax * cs - az * sn
    v[2] += az * cs + ax * sn


def scale_impulse(raw, slow):
    v = raw
    if slow:
        v = f32(v * f32(0.3))
    return f32(v * f32(0.98))


def axis_impulse(positive, negative):
    if positive == negative:
        return f32(0.0)
    return f32(1.0) if positive else f32(-1.0)


def ground_speed(sprinting, friction):
    attr = 0.10000000149011612
    if sprinting:
        attr = attr * (1.0 + 0.30000001192092896)
    attr = f32(attr)
    cube = f32(f32(friction * friction) * friction)
    return f32(attr * f32(f32(0.21600002) / cube))


def boxes_at(block, table, cx, cy, cz):
    b = block(cx, cy, cz)
    if b is None:
        return [(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)]
    if b not in table:
        return []
    out = []
    for bx in table[b]:
        if b in OFFSET_MAX:
            ox, oz = block_offset(cx, cz, OFFSET_MAX[b])
            bx = (bx[0] + ox, bx[1], bx[2] + oz, bx[3] + ox, bx[4], bx[5] + oz)
        out.append(bx)
    return out


def move_axis(block, table, box, axis, d):
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
                c = (cx, cy, cz)
                for sb in boxes_at(block, table, cx, cy, cz):
                    ok = True
                    for o in others:
                        if not (box[o] + 1e-9 < c[o] + sb[o + 3] and c[o] + sb[o] + 1e-9 < box[o + 3]):
                            ok = False
                    if not ok:
                        continue
                    if out > 0.0 and hi <= c[axis] + sb[axis]:
                        out = min(out, c[axis] + sb[axis] - hi)
                    elif out < 0.0 and lo >= c[axis] + sb[axis + 3]:
                        out = max(out, c[axis] + sb[axis + 3] - lo)
    return out


def grid_move(block, table, box, dx, dy, dz):
    b = list(box)
    res = []
    for axis, d in ((1, dy), (0, dx), (2, dz)):
        m = move_axis(block, table, b, axis, d)
        b[axis] += m
        b[axis + 3] += m
        res.append(m)
    return res[1], res[0], res[2]


def overlaps(block, table, box):
    for cx in range(math.floor(box[0]), math.floor(box[3]) + 1):
        for cy in range(math.floor(box[1]) - 1, math.floor(box[4]) + 1):
            for cz in range(math.floor(box[2]), math.floor(box[5]) + 1):
                for sb in boxes_at(block, table, cx, cy, cz):
                    if (box[0] + 1e-9 < cx + sb[3] and cx + sb[0] + 1e-9 < box[3]
                            and box[1] + 1e-9 < cy + sb[4] and cy + sb[1] + 1e-9 < box[4]
                            and box[2] + 1e-9 < cz + sb[5] and cz + sb[2] + 1e-9 < box[5]):
                        return True
    return False


def shifted(box, dx, dy, dz):
    return [box[0] + dx, box[1] + dy, box[2] + dz, box[3] + dx, box[4] + dy, box[5] + dz]


def edge_shrink(d):
    if d < 0.05 and d >= -0.05:
        return 0.0
    return d - 0.05 if d > 0.0 else d + 0.05


def sneak_edge(block, table, box, dx, dz):
    def free(a, b):
        return not overlaps(block, table, shifted(box, a, -STEP_HEIGHT, b))
    d0, d1 = dx, dz
    while d0 != 0.0 and free(d0, 0.0):
        d0 = edge_shrink(d0)
    while d1 != 0.0 and free(0.0, d1):
        d1 = edge_shrink(d1)
    while d0 != 0.0 and d1 != 0.0 and free(d0, d1):
        d0, d1 = edge_shrink(d0), edge_shrink(d1)
    return d0, d1


def move_step(block, table, box, dx, dy, dz, was_ground):
    out = list(grid_move(block, table, box, dx, dy, dz))
    hit_y = out[1] != dy
    flag = was_ground or (hit_y and dy < 0.0)
    hit_x, hit_z = out[0] != dx, out[2] != dz
    if not flag or not (hit_x or hit_z):
        return out[0], out[1], out[2]
    r1 = grid_move(block, table, box, dx, STEP_HEIGHT, dz)
    e = [box[0] + min(dx, 0.0), box[1], box[2] + min(dz, 0.0), box[3] + max(dx, 0.0), box[4], box[5] + max(dz, 0.0)]
    r2 = grid_move(block, table, e, 0.0, STEP_HEIGHT, 0.0)
    sx, sy, sz = r1
    if r2[1] < STEP_HEIGHT:
        r3 = grid_move(block, table, shifted(box, 0.0, r2[1], 0.0), dx, 0.0, dz)
        if r3[0] * r3[0] + r3[2] * r3[2] > sx * sx + sz * sz:
            sx, sy, sz = r3[0], r2[1], r3[2]
    if sx * sx + sz * sz > out[0] * out[0] + out[2] * out[2]:
        r4 = grid_move(block, table, shifted(box, sx, sy, sz), 0.0, dy - sy, 0.0)
        return sx, sy + r4[1], sz
    return out[0], out[1], out[2]


def minor_collision(strafe, forward, yaw, mx, mz):
    r = yaw_rad(yaw)
    sn, cs = float(mth_sin(r)), float(mth_cos(r))
    xa, za = float(strafe), float(forward)
    d2 = xa * cs - za * sn
    d3 = za * cs + xa * sn
    d4 = d2 * d2 + d3 * d3
    d5 = mx * mx + mz * mz
    if d4 < 0.00001 or d5 < 0.00001:
        return False
    c = (d2 * mx + d3 * mz) / math.sqrt(d4 * d5)
    return math.atan2(math.sqrt(max(0.0, 1.0 - c * c)), c) < 0.13962634


def kind_at(block, mvon, x, y, z):
    b = block(x, y, z)
    if not mvon or b is None or b == -2:
        return ""
    return KIND.get(b, "")


def kind_factor(k, jump):
    if jump:
        return 0.5 if k == "honey" else 1.0
    return SPEED_FACTOR if k in ("sand", "honey") else 1.0


def block_factor(block, mvon, x, y, z, jump):
    fx, fz = math.floor(x), math.floor(z)
    f = kind_factor(kind_at(block, mvon, fx, math.floor(y), fz), jump)
    if f != 1.0:
        return f
    return kind_factor(kind_at(block, mvon, fx, math.floor(y - BELOW), fz), jump)


def simulate(block, table, mvon, start, yaw, ticks):
    x, y, z = start
    pitch = 0.0
    v = [0.0, 0.0, 0.0]
    ground = flying = noclip = False
    sprinting = False
    hcol = minor = False
    enough_prev = sneak_prev = False
    sprint_trigger = jump_delay = stuck = 0
    fall = 0.0
    slot = 0
    res = []
    for t in ticks:
        dxm, dym, keys, tf, tn, sd = t
        if tf:
            flying = not flying
            v[1] = 0.0
        if tn:
            noclip = not noclip
        if sd:
            slot = (slot + sd) % 9
        yaw, pitch = look(yaw, pitch, 0.5, dxm, dym)
        sneak_key = bool(keys & 32)
        sprint_key = bool(keys & 64)
        fwd = bool(keys & 1)
        height = 1.5 if sneak_key else 1.8
        slow = sneak_key and not flying
        raw_fwd = axis_impulse(fwd, bool(keys & 2))
        raw_left = axis_impulse(bool(keys & 4), bool(keys & 8))
        strafe = scale_impulse(raw_left, slow)
        forward = scale_impulse(raw_fwd, slow)
        fwd_now = raw_fwd > 0.00001
        pre = f32(raw_fwd * f32(0.3)) if slow else raw_fwd
        enough_now = pre >= f32(0.8)
        hcol_stop = hcol and not minor
        if sprint_trigger > 0:
            sprint_trigger -= 1
        if ground and not sneak_prev and not enough_prev and enough_now and not sprinting:
            if sprint_trigger <= 0 and not sprint_key:
                sprint_trigger = 7
            else:
                sprinting = True
        if not sprinting and enough_now and sprint_key:
            sprinting = True
        if sprinting and (not fwd_now or hcol_stop or slow):
            sprinting = False
        if sneak_key and not flying:
            sprinting = False
        enough_prev, sneak_prev = enough_now, sneak_key
        jump_key = bool(keys & 16)
        if jump_delay > 0:
            jump_delay -= 1
        if not jump_key:
            jump_delay = 0
        if noclip or flying:
            stuck = 0
        v = [0.0 if abs(c) < 0.003 else c for c in v]
        if flying:
            step = (1.0 if keys & 16 else 0.0) - (1.0 if sneak_key else 0.0)
            v[1] += step * float(f32(f32(0.05) * f32(3.0)))
        elif jump_key and ground and jump_delay == 0 and not noclip:
            v[1] = float(f32(f32(0.42) * f32(block_factor(block, mvon, x, y, z, True))))
            if sprinting:
                r = yaw_rad(yaw)
                v[0] += float(f32(f32(0.0 - mth_sin(r)) * f32(0.2)))
                v[2] += float(f32(mth_cos(r) * f32(0.2)))
            jump_delay = 10
        was_ground = ground
        fric = f32(0.6)
        if flying or noclip:
            speed = f32(0.05) * 2 if sprinting else f32(0.05)
            speed = f32(speed)
        elif was_ground:
            speed = ground_speed(sprinting, fric)
        else:
            speed = f32(0.025999999) if sprinting else f32(0.02)
        move_relative(v, strafe, forward, speed, yaw)
        st = stuck
        stuck = 0
        dx, dy, dz = v
        if st and not flying and not noclip:
            mxz, myv = (0.25, STUCK_WEB_Y) if st == 1 else (0.8, 0.75)
            dx, dy, dz = dx * mxz, dy * myv, dz * mxz
            v = [0.0, 0.0, 0.0]
        if noclip:
            x, y, z = x + dx, y + dy, z + dz
            ground = False
            fall = 0.0
            hcol = minor = False
            v[0] *= BASE_DRAG
            v[2] *= BASE_DRAG
            v[1] = v[1] * 0.6 if flying else (v[1] - GRAVITY) * DRAG_Y
            res.append((x, y, z, yaw, pitch, ground, slot))
            continue
        box = [x - 0.3, y, z - 0.3, x + 0.3, y + height, z + 0.3]
        mdx, mdz = dx, dz
        if sneak_key and not flying and (dx != 0.0 or dz != 0.0):
            above = ground or (fall < STEP_HEIGHT and overlaps(block, table, shifted(box, 0.0, fall - STEP_HEIGHT, 0.0)))
            if above:
                mdx, mdz = sneak_edge(block, table, box, dx, dz)
        rx, ry, rz = move_step(block, table, box, mdx, dy, mdz, ground and not flying)
        x, y, z = x + rx, y + ry, z + rz
        hit_x, hit_y, hit_z = rx != mdx, ry != dy, rz != mdz
        ground = hit_y and dy < 0.0
        hcol = hit_x or hit_z
        minor = hcol and minor_collision(strafe, forward, yaw, rx, rz)
        if hit_x:
            v[0] = 0.0
        if hit_z:
            v[2] = 0.0
        if ground or flying:
            fall = 0.0
        elif ry < 0.0:
            fall = fall - ry
        if flying:
            v[1] = dy * 0.6
            v[0] *= BASE_DRAG
            v[2] *= BASE_DRAG
        else:
            if hit_y:
                v[1] = 0.0
            for cz in range(math.floor(z - 0.3 + 1e-7), math.floor(z + 0.3 - 1e-7) + 1):
                for cy in range(math.floor(y + 1e-7), math.floor(y + height - 1e-7) + 1):
                    for cx in range(math.floor(x - 0.3 + 1e-7), math.floor(x + 0.3 - 1e-7) + 1):
                        k = kind_at(block, mvon, cx, cy, cz)
                        if k == "web":
                            stuck, fall = 1, 0.0
                        elif k == "berry":
                            stuck, fall = 2, 0.0
                        elif k == "honey":
                            if (not ground and y <= cy + 0.9375 - 1e-7 and v[1] < -0.08
                                    and (abs(cx + 0.5 - x) + 1e-7 > 0.7375 or abs(cz + 0.5 - z) + 1e-7 > 0.7375)):
                                v[1], fall = -0.05, 0.0
            sf = block_factor(block, mvon, x, y, z, False)
            drag = float(f32(fric * f32(0.91))) if was_ground else BASE_DRAG
            v[0] = v[0] * sf * drag
            v[2] = v[2] * sf * drag
            v[1] = (v[1] - GRAVITY) * DRAG_Y
        res.append((x, y, z, yaw, pitch, ground, slot))
    return res


# ---------------------------------------------------------------- picking (slab test, brute force)
def pick(block, o, d, reach, is_pick=None):
    """is_pick: optional predicate on a block id (default: ids 1 and 2, the test worlds)"""
    ln = math.sqrt((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2])
    d = [d[0] / ln, d[1] / ln, d[2] / ln]

    def pickable(x, y, z):
        b = block(x, y, z)
        if is_pick is not None:
            return b is not None and b >= 0 and is_pick(b)
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
    wa, wb, wc = load_world(d + "/worldA.bin"), load_world(d + "/worldB.bin"), load_world(d + "/worldC.bin")
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
            if name.startswith("slow_"):
                sim = simulate(wc, BOXES_C, not name.endswith("_off"), start, yaw0, ticks)
            else:
                sim = simulate(wa, BOXES_A, False, start, yaw0, ticks)
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
    if nl != 3 or nph != 19 or nray != 600 or nbox != 4 or nhit < 150:
        bad += 1
        print("  DIFF  scene counts look=%d phys=%d rays=%d (hits %d) box=%d" % (nl, nph, nray, nhit, nbox))
    print("  %d problems" % bad)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
