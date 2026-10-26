import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jxlbits import BitWriter, encode_toc, to_bits, to_bytes

KIND_MODE = [0, 1, 2, 3, 0, 0, 0, 0, 0, 4, 5, 0, 0, 0, 0, 0, 0]
KIND_BANDS = [6, 0, 0, 4, 7, 8, 7, 8, 8, 4, 0, 8, 8, 8, 8, 8, 8]
SMALL_KINDS = (1, 2, 3, 9, 10)


def load_tables(root):
    text = open(root + "/lib/jxl/jxl_qwdata.fi").read()
    params = [float(v) for v in re.search(r"static PARAMS: \[f64; \d+\] = \[(.*?)\]", text, re.S).group(1).split(",")]
    offsets = [int(v) for v in re.search(r"static OFFSETS: \[usize; \d+\] = \[(.*?)\]", text, re.S).group(1).split(",")]
    return params, offsets


def put_dct_params(bw, values, nb, first_scale, delta):
    bw.put(nb - 1, 4)
    for c in range(3):
        for i in range(nb):
            v = values[c * nb + i]
            if i == 0:
                v = v * first_scale / 64.0
            else:
                v = v + delta
            bw.put_f16(v)


def emit_kind(bw, kind, custom, params, offsets, first_scale, delta):
    if not custom:
        bw.put(0, 3)
        return
    off = offsets[kind]
    mode = KIND_MODE[kind]
    if mode == 0:
        nb = KIND_BANDS[kind]
        bw.put(6, 3)
        put_dct_params(bw, params[off:off + 3 * nb], nb, first_scale, delta)
    if mode == 1:
        bw.put(1, 3)
        for i in range(9):
            bw.put_f16(params[off + i] / 64.0 * 1.2)
    if mode == 2:
        bw.put(2, 3)
        for i in range(18):
            bw.put_f16(params[off + i] / 64.0 * 1.15)
    if mode == 3:
        bw.put(3, 3)
        for i in range(6):
            bw.put_f16(params[off + 12 + i] * 1.1)
        put_dct_params(bw, params[off:off + 12], 4, first_scale, delta)
    if mode == 4:
        bw.put(4, 3)
        for i in range(3):
            bw.put_f16(params[off + 12 + i] * 1.1)
        put_dct_params(bw, params[off:off + 12], 4, first_scale, delta)
    if mode == 5:
        bw.put(5, 3)
        for c in range(3):
            for i in range(9):
                v = params[off + c * 9 + i]
                bw.put_f16(v / 64.0 * 1.1 if i < 6 else v)
        put_dct_params(bw, params[offsets[9]:offsets[9] + 12], 4, first_scale, delta)
        put_dct_params(bw, params[offsets[3]:offsets[3] + 12], 4, first_scale, delta)


def variant_bits(name, params, offsets):
    bw = BitWriter()
    bw.put(0, 1)
    for kind in range(17):
        small = kind in SMALL_KINDS
        if name == "lib":
            custom = False
        if name == "dct":
            custom = not small
        if name == "small":
            custom = small
        if name == "all":
            custom = True
        emit_kind(bw, kind, custom, params, offsets, 1.3, 0.1)
    return bw.bits


def rebuild(data, probe, new_bits, strip):
    count = probe[0]
    nlf = probe[1]
    secs = [(probe[2 + 2 * i], probe[3 + 2 * i]) for i in range(count)]
    single = count == 1
    idx = 0 if single else 1 + nlf
    start = secs[0][0]
    old_toc = encode_toc([s for _, s in secs])
    prefix = data[:start - len(old_toc)]
    assert data[start - len(old_toc):start] == old_toc
    off, size = secs[idx]
    bits = to_bits(data[off:off + size])
    at = probe[2 + 2 * count] if single else 0
    assert bits[at] == 1
    merged = bits[:at] + new_bits + bits[at + 1:]
    body = to_bytes(merged)
    if strip and body.endswith(b"\x00"):
        body = body[:-1]
    sizes = [s for _, s in secs]
    sizes[idx] = len(body)
    out = prefix + encode_toc(sizes)
    for i, (o, s) in enumerate(secs):
        out += body if i == idx else data[o:o + s]
    return out


def main():
    root, src, dst, variant, strip = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5] == "1"
    probe = [int(v) for v in sys.argv[6:]]
    params, offsets = load_tables(root)
    data = open(src, "rb").read()
    open(dst, "wb").write(rebuild(data, probe, variant_bits(variant, params, offsets), strip))


main()
