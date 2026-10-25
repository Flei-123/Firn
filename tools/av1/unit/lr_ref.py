#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/lr_ref.py <driver> [seeds] -- independent python port of AV1 loop restoration (spec 7.17, 8 bit; literal 4x4-block form of
the spec, no stripes/units shortcuts) and a random-frame comparison against the Firn test driver (lib/av1/av1lr_main.fi): Wiener and
self-guided units with random parameters, all 16 self-guided sets, unit sizes 32..256, 4:4:4 / 4:2:0 / 4:2:2 / 4:0:0, odd sizes, CDEF
output differing from the deblocked frame.  Prints one summary line; exit 1 on the first mismatch."""
import random, struct, subprocess, sys

SGR = [(2, 12, 1, 4), (2, 15, 1, 6), (2, 18, 1, 8), (2, 21, 1, 9), (2, 24, 1, 10), (2, 29, 1, 11), (2, 36, 1, 12), (2, 45, 1, 13),
       (2, 56, 1, 14), (2, 68, 1, 15), (0, 0, 1, 5), (0, 0, 1, 8), (0, 0, 1, 11), (0, 0, 1, 14), (2, 30, 0, 0), (2, 75, 0, 0)]
WMIN, WMAX = [-5, -23, -17], [10, 8, 46]
XMIN, XMAX = [-96, -32], [31, 95]

def r2(x, n):
    return x if n == 0 else (x + (1 << (n - 1))) >> n

def count_units(us, fs):
    return max((fs + (us >> 1)) // us, 1)

def lr_ref(cdef, cur, fw, fh, sx, sy, np_, types, sizes, units):
    out = [[row[:] for row in pl] for pl in cdef]
    for y in range(0, fh, 4):
        for x in range(0, fw, 4):
            for plane in range(np_):
                if types[plane] == 0:
                    continue
                row, col = y >> 2, x >> 2
                subx = sx if plane else 0
                suby = sy if plane else 0
                stripe = (row * 4 + 8) // 64
                sty = (-8 + stripe * 64) >> suby
                eny = sty + (64 >> suby) - 1
                us = sizes[plane]
                urows = count_units(us, r2(fh, suby))
                ucols = count_units(us, r2(fw, subx))
                ur = min(urows - 1, ((row * 4 + 8) >> suby) // us)
                uc = min(ucols - 1, ((col * 4) >> subx) // us)
                pex, pey = r2(fw, subx) - 1, r2(fh, suby) - 1
                bx, by = (col * 4) >> subx, (row * 4) >> suby
                w, h = min(4 >> subx, pex - bx + 1), min(4 >> suby, pey - by + 1)
                u = units[plane][ur * ucols + uc]

                def src(xx, yy):
                    xx = max(0, min(pex, xx))
                    yy = max(0, min(pey, yy))
                    if yy < sty:
                        yy = max(sty - 2, yy)
                        return cur[plane][yy][xx]
                    if yy > eny:
                        yy = min(eny + 2, yy)
                        return cur[plane][yy][xx]
                    return cdef[plane][yy][xx]

                if u["type"] == 1:
                    def filt(co):
                        f = [0] * 7
                        f[3] = 128
                        for i in range(3):
                            f[i] = co[i]
                            f[6 - i] = co[i]
                            f[3] -= 2 * co[i]
                        return f
                    vf, hf = filt(u["w"][0:3]), filt(u["w"][3:6])
                    offset, limit = 1 << (8 + 7 - 3 - 1), (1 << (8 + 1 + 7 - 3)) - 1
                    inter = []
                    for r in range(h + 6):
                        inter.append([])
                        for c in range(w):
                            s = sum(hf[t] * src(bx + c + t - 3, by + r - 3) for t in range(7))
                            inter[r].append(max(-offset, min(limit - offset, r2(s, 3))))
                    for r in range(h):
                        for c in range(w):
                            s = sum(vf[t] * inter[r + t][c] for t in range(7))
                            out[plane][by + r][bx + c] = max(0, min(255, r2(s, 11)))
                elif u["type"] == 2:
                    st = u["set"]
                    def boxf(pas):
                        r = SGR[st][pas * 2]
                        if r == 0:
                            return None
                        eps = SGR[st][pas * 2 + 1]
                        n = (2 * r + 1) ** 2
                        n2e = n * n * eps
                        s = ((1 << 20) + n2e // 2) // n2e
                        A, B = {}, {}
                        for i in range(-1, h + 1):
                            for j in range(-1, w + 1):
                                a = b = 0
                                for dy in range(-r, r + 1):
                                    for dx in range(-r, r + 1):
                                        cc = src(bx + j + dx, by + i + dy)
                                        a += cc * cc
                                        b += cc
                                p = max(0, a * n - b * b)
                                z = r2(p * s, 20)
                                a2 = 256 if z >= 255 else (1 if z == 0 else ((z << 8) + z // 2) // (z + 1))
                                oon = ((1 << 12) + n // 2) // n
                                A[(i, j)] = a2
                                B[(i, j)] = r2((256 - a2) * b * oon, 12)
                        F = []
                        for i in range(h):
                            shift = 4 if (pas == 0 and (i & 1)) else 5
                            F.append([])
                            for j in range(w):
                                a = b = 0
                                for dy in (-1, 0, 1):
                                    for dx in (-1, 0, 1):
                                        if pas == 0:
                                            wt = (6 if dx == 0 else 5) if ((i + dy) & 1) else 0
                                        else:
                                            wt = 4 if (dx == 0 or dy == 0) else 3
                                        a += wt * A[(i + dy, j + dx)]
                                        b += wt * B[(i + dy, j + dx)]
                                v = a * cdef[plane][by + i][bx + j] + b
                                F[i].append(r2(v, 8 + shift - 4))
                        return F
                    f0, f1 = boxf(0), boxf(1)
                    w0, w1 = u["x"]
                    w2 = 128 - w0 - w1
                    for i in range(h):
                        for j in range(w):
                            uu = cdef[plane][by + i][bx + j] << 4
                            v = w1 * uu
                            v += w0 * (f0[i][j] if SGR[st][0] else uu)
                            v += w2 * (f1[i][j] if SGR[st][2] else uu)
                            out[plane][by + i][bx + j] = max(0, min(255, r2(v, 11)))
    return out

def rec_bytes(u):
    b = bytearray(16)
    b[0] = u["type"]
    b[1] = u.get("set", 0)
    for i, v in enumerate(u.get("w", [0] * 6)):
        b[2 + i] = v & 255
    for i, v in enumerate(u.get("x", [0, 0])):
        b[8 + i] = v & 255
    return bytes(b)

def case(driver, rnd, fw, fh, sx, sy, np_, ftypes, ushift, uvshift, mode):
    stride, prow = (fw + 63) & -64, (fh + 63) & -64
    sizes = []
    for p in range(3):
        s = 256 >> (2 - ushift)
        sizes.append(s >> uvshift if p else s)
    units, blob_units = [[], [], []], b""
    for p in range(np_):
        if ftypes[p] == 0:
            continue
        subx = sx if p else 0
        suby = sy if p else 0
        n = count_units(sizes[p], r2(fw, subx)) * count_units(sizes[p], r2(fh, suby))
        for _ in range(n):
            t = rnd.choice([0, 1, 2]) if ftypes[p] == 3 else (rnd.choice([0, ftypes[p], ftypes[p]]))
            u = {"type": t}
            if t == 1:
                u["w"] = [0 if (p and k % 3 == 0) else rnd.randint(WMIN[k % 3], WMAX[k % 3]) for k in range(6)]
            elif t == 2:
                u["set"] = rnd.randrange(16)
                u["x"] = [rnd.randint(XMIN[0], XMAX[0]), rnd.randint(XMIN[1], XMAX[1])]
            units[p].append(u)
            blob_units += rec_bytes(u)
    def mk():
        planes = []
        for p in range(3):
            pl = []
            for y in range(prow):
                if mode == 0:
                    pl.append([rnd.randrange(256) for _ in range(stride)])
                elif mode == 1:
                    pl.append([max(0, min(255, 100 + (x * 2 + y * 3) % 90 + (rnd.randrange(-25, 26) if rnd.random() < 0.3 else 0))) for x in range(stride)])
                else:
                    pl.append([210 if (x // 3 + y // 5) % 2 else 35 for x in range(stride)])
            planes.append(pl)
        return planes
    cur = mk()
    cdef = [[[max(0, min(255, v + rnd.randint(-6, 6))) if rnd.random() < 0.5 else v for v in row] for row in pl] for pl in cur]
    head = struct.pack("<12i", np_, sx, sy, fw, fh, ftypes[0], ftypes[1], ftypes[2], ushift, uvshift, stride, prow)
    flat = lambda ps: b"".join(bytes(v for row in pl for v in row) for pl in ps)
    res = subprocess.run([driver], input=head + blob_units + flat(cdef) + flat(cur), capture_output=True)
    assert res.returncode == 0, "driver failed"
    ref = lr_ref(cdef, cur, fw, fh, sx, sy, np_, ftypes, sizes, units)
    return res.stdout == flat(ref)

def main():
    driver = sys.argv[1]
    seeds = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    n = 0
    for seed in range(seeds):
        rnd = random.Random(2000 + seed)
        # (width, height, subx, suby, planes, frame types, unit shift, uv shift)
        for (fw, fh, sx, sy, np_, ft, us, uvs) in [
                (150, 90, 0, 0, 3, [1, 2, 3], 0, 0), (130, 133, 0, 0, 3, [3, 3, 3], 0, 0), (97, 71, 1, 1, 3, [2, 1, 3], 0, 1),
                (131, 77, 1, 1, 3, [3, 3, 2], 0, 0), (70, 66, 1, 0, 3, [1, 3, 2], 0, 0), (5, 7, 0, 0, 3, [3, 3, 3], 0, 0),
                (200, 140, 0, 0, 1, [3, 0, 0], 1, 0), (260, 70, 0, 0, 3, [2, 2, 1], 2, 0), (75, 200, 1, 1, 3, [1, 1, 1], 1, 1),
                (66, 130, 0, 0, 3, [3, 0, 3], 0, 0)]:
            for mode in (0, 1, 2):
                if not case(driver, rnd, fw, fh, sx, sy, np_, ft, us, uvs, mode):
                    print("lr_ref: MISMATCH %dx%d sub=%d%d planes=%d types=%s ushift=%d uvshift=%d mode=%d seed=%d" % (fw, fh, sx, sy, np_, ft, us, uvs, mode, seed))
                    sys.exit(1)
                n += 1
    print("lr_ref: ok (%d random frames match the python spec port)" % n)

main()
