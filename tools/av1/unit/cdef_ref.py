#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/cdef_ref.py <driver> [seeds] -- independent python port of AV1 CDEF (spec 7.15, 8 bit) and a random-frame comparison against
the Firn test driver (lib/av1/av1cdef_main.fi): skip maps, cdef_idx = -1 blocks, 4:4:4 / 4:2:0 / 4:2:2 / 4:0:0, odd mi sizes, all strengths.
Prints one line per case; exit 1 on the first mismatch."""
import random, struct, subprocess, sys

UVDIR = [[[0, 1, 2, 3, 4, 5, 6, 7], [1, 2, 2, 2, 3, 4, 6, 0]], [[7, 0, 2, 4, 5, 6, 6, 6], [0, 1, 2, 3, 4, 5, 6, 7]]]
DIVT = [0, 840, 420, 280, 210, 168, 140, 120, 105]
DIRS = [[[-1, 1], [-2, 2]], [[0, 1], [-1, 2]], [[0, 1], [0, 2]], [[0, 1], [1, 2]], [[1, 1], [2, 2]], [[1, 0], [2, 1]], [[1, 0], [2, 0]], [[1, 0], [2, -1]]]
PRI = [[4, 2], [3, 3]]
SEC = [[2, 1], [2, 1]]

def flog2(v):
    return v.bit_length() - 1

def constrain(diff, thr, damping):
    if not thr:
        return 0
    adj = max(0, damping - flog2(thr))
    mag = min(abs(diff), max(0, thr - (abs(diff) >> adj)))
    return -mag if diff < 0 else mag

def direction(cur, x0, y0):
    cost = [0] * 8
    part = [[0] * 15 for _ in range(8)]
    for i in range(8):
        for j in range(8):
            x = cur[0][y0 + i][x0 + j] - 128
            part[0][i + j] += x
            part[1][i + j // 2] += x
            part[2][i] += x
            part[3][3 + i - j // 2] += x
            part[4][7 + i - j] += x
            part[5][3 - i // 2 + j] += x
            part[6][j] += x
            part[7][i // 2 + j] += x
    for i in range(8):
        cost[2] += part[2][i] ** 2
        cost[6] += part[6][i] ** 2
    cost[2] *= DIVT[8]
    cost[6] *= DIVT[8]
    for i in range(7):
        cost[0] += (part[0][i] ** 2 + part[0][14 - i] ** 2) * DIVT[i + 1]
        cost[4] += (part[4][i] ** 2 + part[4][14 - i] ** 2) * DIVT[i + 1]
    cost[0] += part[0][7] ** 2 * DIVT[8]
    cost[4] += part[4][7] ** 2 * DIVT[8]
    for i in range(1, 8, 2):
        for j in range(5):
            cost[i] += part[i][3 + j] ** 2
        cost[i] *= DIVT[8]
        for j in range(3):
            cost[i] += (part[i][j] ** 2 + part[i][10 - j] ** 2) * DIVT[2 * j + 2]
    best, d = 0, 0
    for i in range(8):
        if cost[i] > best:
            best, d = cost[i], i
    return d, (best - cost[(d + 4) & 7]) >> 10

def cdef_ref(cur, mc, mr, sx, sy, np_, damp, par, skp, idxmap, sbcols):
    out = [[row[:] for row in pl] for pl in cur]
    def filt(plane, r, c, pri, sec, damping, d):
        subx = sx if plane else 0
        suby = sy if plane else 0
        x0, y0 = (c * 4) >> subx, (r * 4) >> suby
        w, h = 8 >> subx, 8 >> suby
        def at(i, j, dd, k, sign):
            y = y0 + i + sign * DIRS[dd][k][0]
            x = x0 + j + sign * DIRS[dd][k][1]
            if y < 0 or x < 0 or ((y << suby) >> 2) >= mr or ((x << subx) >> 2) >= mc:
                return None
            return cur[plane][y][x]
        for i in range(h):
            for j in range(w):
                x = cur[plane][y0 + i][x0 + j]
                s, mx, mn = 0, x, x
                for k in range(2):
                    for sign in (-1, 1):
                        p = at(i, j, d, k, sign)
                        if p is not None:
                            s += PRI[pri & 1][k] * constrain(p - x, pri, damping)
                            mx, mn = max(p, mx), min(p, mn)
                        for off in (-2, 2):
                            q = at(i, j, (d + off) & 7, k, sign)
                            if q is not None:
                                s += SEC[pri & 1][k] * constrain(q - x, sec, damping)
                                mx, mn = max(q, mx), min(q, mn)
                out[plane][y0 + i][x0 + j] = max(mn, min(mx, x + ((8 + s - (1 if s < 0 else 0)) >> 4)))
    for r in range(0, mr, 2):
        for c in range(0, mc, 2):
            idx = idxmap[(r >> 4) * sbcols + (c >> 4)] - 1
            if idx < 0:
                continue
            if skp[r * mc + c] and skp[r * mc + c + 1] and skp[(r + 1) * mc + c] and skp[(r + 1) * mc + c + 1]:
                continue
            yd, var = direction(cur, c * 4, r * 4)
            pri, sec = par[idx], par[8 + idx]
            d = 0 if pri == 0 else yd
            vs = min(flog2(var >> 6), 12) if (var >> 6) else 0
            pri = ((pri * (4 + vs) + 8) >> 4) if var else 0
            filt(0, r, c, pri, sec, damp, d)
            if np_ > 1:
                upri, usec = par[16 + idx], par[24 + idx]
                ud = 0 if upri == 0 else UVDIR[sx][sy][yd]
                filt(1, r, c, upri, usec, damp - 1, ud)
                filt(2, r, c, upri, usec, damp - 1, ud)
    return out

def case(driver, rnd, mc, mr, sx, sy, np_, mode):
    acols, arows = (mc + 15) & -16, (mr + 15) & -16
    stride, prow = acols * 4, arows * 4
    damp = rnd.randrange(3, 7)
    par = [rnd.randrange(16) if rnd.random() < 0.85 else 0 for _ in range(8)] + [rnd.choice([0, 1, 2, 4]) for _ in range(8)] + \
          [rnd.randrange(16) if rnd.random() < 0.85 else 0 for _ in range(8)] + [rnd.choice([0, 1, 2, 4]) for _ in range(8)]
    planes = []
    for p in range(3):
        pl = []
        for y in range(prow):
            if mode == 0:    # noise
                pl.append([rnd.randrange(256) for _ in range(stride)])
            elif mode == 1:  # smooth + sparse noise
                pl.append([max(0, min(255, 100 + (x * 2 + y * 3) % 90 + (rnd.randrange(-25, 26) if rnd.random() < 0.2 else 0))) for x in range(stride)])
            else:            # stripes + edges
                pl.append([200 if (x // 3 + y // 5) % 2 else 40 for x in range(stride)])
        planes.append(pl)
    skp = [1 if rnd.random() < 0.4 else 0 for _ in range(mc * mr)]
    sbcols, sbrows = acols >> 4, arows >> 4
    idxmap = [rnd.choice([0, 1, 2, 3, 4, 5, 6, 7, 8]) for _ in range(sbcols * sbrows)]
    blob = struct.pack("<8i", mc, mr, sx, sy, np_, damp, 0, 0) + bytes(par) + b"".join(bytes(v for row in pl for v in row) for pl in planes) + bytes(skp) + bytes(idxmap)
    res = subprocess.run([driver], input=blob, capture_output=True)
    assert res.returncode == 0, "driver failed"
    ref = cdef_ref(planes, mc, mr, sx, sy, np_, damp, par, skp, idxmap, sbcols)
    refb = b"".join(bytes(v for row in pl for v in row) for pl in ref)
    return res.stdout == refb

def main():
    driver = sys.argv[1]
    seeds = int(sys.argv[2]) if len(sys.argv) > 2 else 2
    n = 0
    for seed in range(seeds):
        rnd = random.Random(1000 + seed)
        for (mc, mr, sx, sy, np_) in [(16, 16, 0, 0, 3), (18, 10, 1, 1, 3), (6, 14, 1, 0, 3), (34, 20, 0, 0, 1), (12, 12, 1, 1, 3), (2, 2, 0, 0, 3), (20, 36, 1, 1, 3)]:
            for mode in (0, 1, 2):
                if not case(driver, rnd, mc, mr, sx, sy, np_, mode):
                    print("cdef_ref: MISMATCH mc=%d mr=%d sub=%d%d planes=%d mode=%d seed=%d" % (mc, mr, sx, sy, np_, mode, seed))
                    sys.exit(1)
                n += 1
    print("cdef_ref: ok (%d random frames match the python spec port)" % n)

main()
