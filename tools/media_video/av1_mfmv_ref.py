import sys

DIV = [0, 16384, 8192, 5461, 4096, 3276, 2730, 2340, 2048, 1820, 1638, 1489, 1365, 1260, 1170, 1092, 1024, 963, 910, 862, 819, 780, 744, 712, 682, 655, 630, 606, 585, 564, 546, 528]
MAXD = 31


class Rng:
    def __init__(self, x):
        self.x = x

    def next(self):
        self.x = (self.x * 1103515245 + 12345) & 2147483647
        return self.x >> 8


def rel(bits, a, b):
    if bits == 0:
        return 0
    d = a - b
    m = 1 << (bits - 1)
    return (d & (m - 1)) - (d & m)


def proj(v, num, den):
    den = min(den, MAXD)
    num = min(num, MAXD) if num > 0 else max(num, -MAXD)
    r = v * num * DIV[den]
    r = (r + 8192) >> 14 if r >= 0 else -((-r + 8192) >> 14)
    return max(-16383, min(16383, r))


def trunc_shift(v):
    return v >> 6 if v >= 0 else -((-v) >> 6)


def scenario(seed):
    r = Rng(seed * 7919 + 17)
    bits = 4 + r.next() % 4
    mi_rows = 2 * (4 + r.next() % 36)
    mi_cols = 2 * (4 + r.next() % 36)
    cur = r.next() % (1 << bits)
    idx = [r.next() % 8 for _ in range(7)]
    slots = []
    for _ in range(8):
        sl = {}
        sl['hint'] = r.next() % (1 << bits)
        ft = r.next() % 8
        sl['type'] = 0 if ft == 0 else (2 if ft == 1 else 1)
        ok = r.next() % 6
        sl['rows'] = mi_rows + 2 if ok == 0 else mi_rows
        sl['cols'] = mi_cols
        sl['saved'] = [0] + [r.next() % (1 << bits) for _ in range(7)]
        has_mf = r.next() % 10
        pick = r.next() % 3
        rng_mv = [64, 600, 4095][pick]
        cells = ((sl['rows'] + 1) >> 1) * ((sl['cols'] + 1) >> 1)
        mf = []
        for _ in range(cells):
            present = r.next() % 4
            rf = r.next() % 8
            if present == 0:
                rf = 0
            mvr = r.next() % (2 * rng_mv + 1) - rng_mv
            mvc = r.next() % (2 * rng_mv + 1) - rng_mv
            mf.append((rf, mvr, mvc))
        sl['mf'] = None if has_mf == 0 else mf
        slots.append(sl)
    order_hints = [0] + [slots[idx[i]]['hint'] for i in range(7)]
    return bits, mi_rows, mi_cols, cur, idx, slots, order_hints


def project(st, start_ref, dir_, tpl):
    bits, mi_rows, mi_cols, cur, idx, slots, oh = st
    sl = slots[idx[start_ref - 1]]
    if sl['mf'] is None:
        return 0
    if sl['type'] in (0, 2):
        return 0
    if sl['rows'] != mi_rows or sl['cols'] != mi_cols:
        return 0
    ref_offset = [0] + [rel(bits, sl['hint'], sl['saved'][rf]) for rf in range(1, 8)]
    s2c = rel(bits, sl['hint'], cur)
    if dir_ == 2:
        s2c = -s2c
    rows8 = (mi_rows + 1) >> 1
    cols8 = (mi_cols + 1) >> 1
    for br in range(rows8):
        for bc in range(cols8):
            rf, mvr, mvc = sl['mf'][br * cols8 + bc]
            if rf > 0:
                off = ref_offset[rf]
                ok = abs(off) <= MAXD and off > 0 and abs(s2c) <= MAXD
                if ok:
                    pr = proj(mvr, s2c, off)
                    pc = proj(mvc, s2c, off)
                    ro = trunc_shift(pr)
                    co = trunc_shift(pc)
                    row = br - ro if dir_ == 2 else br + ro
                    col = bc - co if dir_ == 2 else bc + co
                    bbr = (br >> 3) << 3
                    bbc = (bc >> 3) << 3
                    if row < 0 or row >= (mi_rows >> 1) or col < 0 or col >= (mi_cols >> 1):
                        ok = False
                    elif row < bbr or row >= bbr + 8 or col < bbc - 8 or col >= bbc + 16:
                        ok = False
                    if ok:
                        tpl[row * (mi_cols >> 1) + col] = (mvr, mvc, off)
    return 1


def estimate(st):
    bits, mi_rows, mi_cols, cur, idx, slots, oh = st
    cells = (mi_rows >> 1) * (mi_cols >> 1)
    tpl = [None] * cells
    stamp = 1
    last = slots[idx[0]]
    if last['saved'][7] != oh[4]:
        project(st, 1, 2, tpl)
    if rel(bits, oh[5], cur) > 0:
        if project(st, 5, 0, tpl):
            stamp -= 1
    if rel(bits, oh[6], cur) > 0:
        if project(st, 6, 0, tpl):
            stamp -= 1
    if rel(bits, oh[7], cur) > 0 and stamp >= 0:
        if project(st, 7, 0, tpl):
            stamp -= 1
    if stamp >= 0:
        project(st, 2, 2, tpl)
    return tpl


def main():
    out = []
    for seed in range(1, 301):
        st = scenario(seed)
        bits, mi_rows, mi_cols, cur, idx, slots, oh = st
        tpl = estimate(st)
        for ref in range(1, 8):
            to_dst = rel(bits, cur, oh[ref])
            h = 0
            count = 0
            for c in tpl:
                if c is None:
                    a = b = -32768
                else:
                    a = proj(c[0], to_dst, c[2])
                    b = proj(c[1], to_dst, c[2])
                    count += 1
                h = (h * 1000003 + (a & 65535)) & 1099511627775
                h = (h * 1000003 + (b & 65535)) & 1099511627775
            out.append('%d %d %d %d' % (seed, ref, count, h))
    with open(sys.argv[1], 'w') as f:
        f.write('\n'.join(out) + '\n')


main()
