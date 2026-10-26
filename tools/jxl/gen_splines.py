import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jxlbits import BitWriter, encode_toc, to_bits, to_bytes

CL_ORDER = [1, 2, 3, 4, 0, 5, 17, 6, 16, 7, 8, 9, 10, 11, 12, 13, 14, 15]

SETS = {
    "a": {
        "quant_adj": 0,
        "splines": [
            {
                "points": [(8, 10), (25, 22), (40, 12), (55, 30)],
                "color": [[1, 2, 0, 1], [10, 3, -2, 1, 0, 1], [-3, 1]],
                "sigma": [8, 2, 1],
            },
            {
                "points": [(30, 40), (30, 52)],
                "color": [[0, 1], [-8, 2], [2, -1]],
                "sigma": [5, -1],
            },
        ],
    },
    "b": {
        "quant_adj": 3,
        "splines": [
            {
                "points": [(5, 5), (60, 60)],
                "color": [[2, 0, 1], [12, -2, 3, 1], [4]],
                "sigma": [10, 1, -1, 1],
            },
            {
                "points": [(20, 30)],
                "color": [[0], [14], [-4]],
                "sigma": [9],
            },
            {
                "points": [(50, 8), (44, 20), (52, 34), (40, 44), (46, 58)],
                "color": [[-2, 1], [-9, 2, 1], [3, 0, 1]],
                "sigma": [7, 2],
            },
        ],
    },
}


def pack(v):
    return 2 * v if v >= 0 else -2 * v - 1


def put_value(bw, v):
    if v < 16:
        sym = v
        extra = None
    else:
        n = v.bit_length() - 1
        sym = n + 12
        extra = (n, v - (1 << n))
    for i in range(4, -1, -1):
        bw.put((sym >> i) & 1, 1)
    if extra:
        bw.put(extra[1], extra[0])


def put_entropy_header(bw):
    bw.put(0, 1)
    bw.put(1, 1)
    bw.put(0, 2)
    bw.put(1, 1)
    bw.put(4, 4)
    bw.put(0, 3)
    bw.put(0, 3)
    bw.put(1, 1)
    bw.put(4, 4)
    bw.put(15, 4)
    bw.put(0, 2)
    for sym in CL_ORDER:
        if sym == 5:
            bw.put(7, 4)
        else:
            bw.put(0, 2)


def pad32(values):
    return list(values) + [0] * (32 - len(values))


def spline_bits(spec):
    bw = BitWriter()
    put_entropy_header(bw)
    splines = spec["splines"]
    put_value(bw, len(splines) - 1)
    last = (0, 0)
    for i, sp in enumerate(splines):
        x, y = sp["points"][0]
        if i == 0:
            put_value(bw, x)
            put_value(bw, y)
        else:
            put_value(bw, pack(x - last[0]))
            put_value(bw, pack(y - last[1]))
        last = (x, y)
    put_value(bw, pack(spec["quant_adj"]))
    for sp in splines:
        pts = sp["points"]
        put_value(bw, len(pts) - 1)
        prev = pts[0]
        prev_delta = (0, 0)
        for p in pts[1:]:
            delta = (p[0] - prev[0], p[1] - prev[1])
            put_value(bw, pack(delta[0] - prev_delta[0]))
            put_value(bw, pack(delta[1] - prev_delta[1]))
            prev_delta = delta
            prev = p
        for ch in sp["color"]:
            for v in pad32(ch):
                put_value(bw, pack(v))
        for v in pad32(sp["sigma"]):
            put_value(bw, pack(v))
    return bw.bits


def rebuild(data, probe, bits):
    count = probe[0]
    fs = probe[-2]
    hdr_end = probe[-1]
    secs = [(probe[2 + 2 * i], probe[3 + 2 * i]) for i in range(count)]
    assert data[:2] == b"\xff\x0a"
    old_toc = encode_toc([s for _, s in secs])
    start = secs[0][0]
    hdr = to_bits(data)[fs * 8:hdr_end]
    assert hdr[0] == 0 and hdr[4] == 0 and hdr[5] == 0
    new_hdr = hdr[:4] + [1, 0, 1, 1, 1, 1] + hdr[6:] + [0]
    prefix = data[:fs] + to_bytes(new_hdr)
    assert data[start - len(old_toc):start] == old_toc
    off, size = secs[0]
    merged = bits + to_bits(data[off:off + size])
    body = to_bytes(merged)
    sizes = [s for _, s in secs]
    sizes[0] = len(body)
    out = prefix + encode_toc(sizes)
    for i, (o, s) in enumerate(secs):
        out += body if i == 0 else data[o:o + s]
    return out


def main():
    src, dst, name = sys.argv[1], sys.argv[2], sys.argv[3]
    probe = [int(v) for v in sys.argv[4:]]
    data = open(src, "rb").read()
    open(dst, "wb").write(rebuild(data, probe, spline_bits(SETS[name])))


main()
