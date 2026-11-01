import re
import sys
import os

filter_src = sys.argv[1]
out_path = sys.argv[2]

NAMES = ("sub_pel_filters_8", "sub_pel_filters_8lp", "sub_pel_filters_8s", "bilinear_filters")


def load_filters(path):
    text = open(path).read()
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    out = []
    for n in NAMES:
        m = re.search(r"\b" + n + r"\b[^=;{]*=\s*\{", text)
        assert m, n
        depth = 0
        pos = m.end() - 1
        start = pos
        while True:
            if text[pos] == "{":
                depth += 1
            elif text[pos] == "}":
                depth -= 1
                if depth == 0:
                    break
            pos += 1
        nums = [int(x) for x in re.findall(r"-?\d+", text[start:pos + 1])]
        assert len(nums) == 128, (n, len(nums))
        out.append([nums[i * 8:(i + 1) * 8] for i in range(16)])
    return out


FILT = load_filters(filter_src)


class Rng:
    def __init__(self, seed):
        self.s = seed

    def rnd(self, n):
        self.s = (self.s * 1103515245 + 12345) & 2147483647
        return ((self.s >> 16) & 32767) % n


class Buf:
    def __init__(self, w, h, rng=None):
        aw = (w + 63) // 64 * 64
        ah = (h + 63) // 64 * 64
        self.w = w
        self.h = h
        self.ys = aw + 16
        self.uvs = aw // 2 + 16
        self.y = bytearray(self.ys * (ah + 2))
        self.u = bytearray(self.uvs * (ah // 2 + 2))
        self.v = bytearray(self.uvs * (ah // 2 + 2))
        if rng is not None:
            for p in (self.y, self.u, self.v):
                for i in range(len(p)):
                    p[i] = rng.rnd(256)

    def plane(self, i):
        if i == 0:
            return self.y, self.ys, self.w, self.h
        return (self.u if i == 1 else self.v), self.uvs, (self.w + 1) >> 1, (self.h + 1) >> 1


def clip(v):
    return 0 if v < 0 else 255 if v > 255 else v


def cdiv(a, b):
    return -((-a) // b) if a < 0 else a // b


def fetch(pl, stride, cw, ch, x, y):
    x = min(max(x, 0), cw - 1)
    y = min(max(y, 0), ch - 1)
    return pl[y * stride + x]


def convolve_ref(pl, stride, cw, ch, x0, y0, sx, sy, xs, ys, w, h, k):
    ih = (((h - 1) * ys + sy) >> 4) + 8
    tmp = []
    for r in range(ih):
        yy = y0 - 3 + r
        row = []
        xq = sx
        for x in range(w):
            s = 0
            for t in range(8):
                s += fetch(pl, stride, cw, ch, x0 + (xq >> 4) - 3 + t, yy) * FILT[k][xq & 15][t]
            row.append(clip((s + 64) >> 7))
            xq += xs
        tmp.append(row)
    out = [[0] * w for _ in range(h)]
    for x in range(w):
        yq = sy
        for y in range(h):
            s = 0
            for t in range(8):
                s += tmp[(yq >> 4) + t][x] * FILT[k][yq & 15][t]
            out[y][x] = clip((s + 64) >> 7)
            yq += ys
    return out


class Scale:
    def __init__(self, rw, rh, w, h):
        assert 2 * w >= rw and 2 * h >= rh and w <= 16 * rw and h <= 16 * rh
        self.xfp = (rw << 14) // w
        self.yfp = (rh << 14) // h
        self.xstep = (16 * self.xfp) >> 14
        self.ystep = (16 * self.yfp) >> 14
        self.scaled = self.xfp != 16384 or self.yfp != 16384

    def sx(self, v):
        return (v * self.xfp) >> 14

    def sy(self, v):
        return (v * self.yfp) >> 14


def to_i16(v):
    v &= 0xFFFF
    return v - 65536 if v >= 32768 else v


def clampv(v, lo, hi):
    return lo if v < lo else hi if v > hi else v


def build(dst, dstride, dorg, refpl, sf, mi, ss, bw, bh, x, y, w, h, mv_row, mv_col, k, avg):
    mi_row, mi_col, bw8, bh8, mi_rows, mi_cols = mi
    pl, stride, cw, ch = refpl
    mi_x = mi_col * 8
    mi_y = mi_row * 8
    to_left = -mi_col * 64
    to_right = (mi_cols - bw8 - mi_col) * 64
    to_top = -mi_row * 64
    to_bottom = (mi_rows - bh8 - mi_row) * 64
    xstart = mi_x >> ss
    ystart = mi_y >> ss
    mul = 1 << (1 - ss)
    if sf.scaled:
        spel_l = (4 + bw) << 4
        spel_t = (4 + bh) << 4
        qc = clampv(to_i16(mv_col * mul), to_left * mul - spel_l, to_right * mul + spel_l - 16)
        qr = clampv(to_i16(mv_row * mul), to_top * mul - spel_t, to_bottom * mul + spel_t - 16)
        x0 = sf.sx(xstart + x)
        y0 = sf.sy(ystart + y)
        smc = sf.sx(qc) + (sf.sx((mi_x + x) << 4) & 15)
        smr = sf.sy(qr) + (sf.sy((mi_y + y) << 4) & 15)
        xs, ys = sf.xstep, sf.ystep
    else:
        x0 = xstart + x
        y0 = ystart + y
        smc = mv_col * mul
        smr = mv_row * mul
        xs = ys = 16
    out = convolve_ref(pl, stride, cw, ch, x0 + (smc >> 4), y0 + (smr >> 4), smc & 15, smr & 15, xs, ys, w, h, k)
    for j in range(h):
        for i in range(w):
            p = dorg + (y + j) * dstride + x + i
            dst[p] = (dst[p] + out[j][i] + 1) >> 1 if avg else out[j][i]


def fnv(h, b):
    return ((h ^ b) * 16777619) & 0xFFFFFFFF


REFDIMS = [(37, 29), (63, 45), (19, 15), (74, 58)]
CUR_W, CUR_H = 37, 29
MI_COLS, MI_ROWS = 5, 4
BW4 = [1, 1, 2, 2, 2, 4, 4, 4, 8, 8, 8, 16, 16]
BH4 = [1, 2, 1, 2, 4, 2, 4, 8, 4, 8, 16, 8, 16]
N1 = 160
N2 = 48


def case1(c):
    rng = Rng(c * 7919 + 1)
    rw, rh = REFDIMS[c % 4]
    ref = Buf(rw, rh, rng)
    cur = Buf(CUR_W, CUR_H, rng)
    plane = rng.rnd(3)
    ss = 1 if plane > 0 else 0
    bw8 = 1 << rng.rnd(4)
    bh8 = 1 << rng.rnd(4)
    mi_col = rng.rnd(MI_COLS) // bw8 * bw8
    mi_row = rng.rnd(MI_ROWS) // bh8 * bh8
    n4w = (bw8 * 2) >> ss
    n4h = (bh8 * 2) >> ss
    bw = n4w * 4
    bh = n4h * 4
    x = y = 0
    w, h = bw, bh
    if bw8 == 1 and bh8 == 1 and rng.rnd(2) == 0:
        x = 4 * rng.rnd(n4w)
        y = 4 * rng.rnd(n4h)
        w = h = 4
    mv_row = rng.rnd(801) - 400
    mv_col = rng.rnd(801) - 400
    if rng.rnd(4) == 0:
        mv_row = rng.rnd(4001) - 2000
        mv_col = rng.rnd(4001) - 2000
    k = rng.rnd(4)
    avg = rng.rnd(2) == 1
    sf = Scale(rw, rh, CUR_W, CUR_H)
    dpl, dstride, _, _ = cur.plane(plane)
    dorg = ((mi_row * 8) >> ss) * dstride + ((mi_col * 8) >> ss)
    build(dpl, dstride, dorg, ref.plane(plane), sf, (mi_row, mi_col, bw8, bh8, MI_ROWS, MI_COLS), ss,
          bw, bh, x, y, w, h, mv_row, mv_col, k, avg)
    hh = 2166136261
    for j in range(h):
        for i in range(w):
            hh = fnv(hh, dpl[dorg + (y + j) * dstride + x + i])
    return hh


def bsize_dims(b):
    return max(1, BW4[b] // 2), max(1, BH4[b] // 2)


def case2(c):
    rng = Rng(c * 104729 + 3)
    ka = rng.rnd(4)
    kb = rng.rnd(4)
    refs = [Buf(REFDIMS[ka][0], REFDIMS[ka][1], rng), Buf(REFDIMS[kb][0], REFDIMS[kb][1], rng)]
    cur = Buf(CUR_W, CUR_H, rng)
    bsize = rng.rnd(13)
    bw8, bh8 = bsize_dims(bsize)
    mi_col = rng.rnd(MI_COLS) // bw8 * bw8
    mi_row = rng.rnd(MI_ROWS) // bh8 * bh8
    ref0 = 1 + rng.rnd(3)
    ref1 = 0
    if rng.rnd(2) == 1:
        ref1 = 1 + rng.rnd(3)
    k = rng.rnd(4)
    mv = [[rng.rnd(801) - 400, rng.rnd(801) - 400] for _ in range(2)]
    bmi = [[[rng.rnd(801) - 400, rng.rnd(801) - 400] for _ in range(2)] for _ in range(4)]
    frames = [ref0] + ([ref1] if ref1 > 0 else [])
    sub8 = bsize < 3
    pool_of = {1: 0, 2: 1, 3: 0}
    for r, fr in enumerate(frames):
        rb = refs[pool_of[fr]]
        sf = Scale(rb.w, rb.h, CUR_W, CUR_H)
        for plane in range(3):
            ss = 1 if plane > 0 else 0
            dpl, dstride, _, _ = cur.plane(plane)
            dorg = ((mi_row * 8) >> ss) * dstride + ((mi_col * 8) >> ss)
            n4w = (bw8 * 2) >> ss
            n4h = (bh8 * 2) >> ss
            miinfo = (mi_row, mi_col, bw8, bh8, MI_ROWS, MI_COLS)
            if sub8:
                i = 0
                for by in range(n4h):
                    for bx in range(n4w):
                        if ss == 0:
                            mr, mc = bmi[i][r]
                        else:
                            mr = rq4(sum(bmi[b][r][0] for b in range(4)))
                            mc = rq4(sum(bmi[b][r][1] for b in range(4)))
                        build(dpl, dstride, dorg, rb.plane(plane), sf, miinfo, ss, n4w * 4, n4h * 4,
                              bx * 4, by * 4, 4, 4, mr, mc, k, r == 1)
                        i += 1
            else:
                build(dpl, dstride, dorg, rb.plane(plane), sf, miinfo, ss, n4w * 4, n4h * 4, 0, 0,
                      n4w * 4, n4h * 4, mv[r][0], mv[r][1], k, r == 1)
    hh = 2166136261
    for plane in range(3):
        ss = 1 if plane > 0 else 0
        dpl, dstride, _, _ = cur.plane(plane)
        dorg = ((mi_row * 8) >> ss) * dstride + ((mi_col * 8) >> ss)
        pw = ((bw8 * 2) >> ss) * 4
        ph = ((bh8 * 2) >> ss) * 4
        for j in range(ph):
            for i in range(pw):
                hh = fnv(hh, dpl[dorg + j * dstride + i])
    return hh


def rq4(v):
    return cdiv(v - 2 if v < 0 else v + 2, 4)


def table(name, vals):
    lines = []
    for i in range(0, len(vals), 8):
        lines.append("    " + ", ".join(str(v) for v in vals[i:i + 8]) + ",")
    return "static %s: [u32; %d] = [\n%s\n]\n" % (name, len(vals), "\n".join(lines))


if __name__ == "__main__":
    e1 = [case1(c) for c in range(N1)]
    e2 = [case2(c) for c in range(N2)]
    tpl = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "vp9_mc_test.tpl")).read()
    tpl = tpl.replace("@EXPECT1@", table("EXPECT1", e1)).replace("@EXPECT2@", table("EXPECT2", e2))
    tpl = tpl.replace("@N1@", str(N1)).replace("@N2@", str(N2))
    open(out_path, "w").write(tpl)
    print("ok", N1, N2)
