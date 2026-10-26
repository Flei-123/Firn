import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import acswriter
import gen_dequant
from jxlbits import encode_toc, to_bits, to_bytes


def rebuild(data, probe, layout, qf_raw, variant, params, offsets, seed):
    count, nlf = probe[0], probe[1]
    assert count == 1
    secs = [(probe[2 + 2 * i], probe[3 + 2 * i]) for i in range(count)]
    lf_end, ac_meta_at, num_contexts = probe[2 + 2 * count], probe[3 + 2 * count], probe[4 + 2 * count]
    start = secs[0][0]
    old_toc = encode_toc([s for _, s in secs])
    prefix = data[:start - len(old_toc)]
    assert data[start - len(old_toc):start] == old_toc
    off, size = secs[0]
    bits = to_bits(data[off:off + size])
    dequant = gen_dequant.variant_bits(variant, params, offsets)
    merged = (
        bits[:ac_meta_at]
        + acswriter.ac_meta_bits(layout, qf_raw)
        + acswriter.hf_global_bits(dequant, num_contexts)
        + acswriter.ac_group_bits(layout, seed, num_contexts)
    )
    body = to_bytes(merged)
    return prefix + encode_toc([len(body)]) + body


def main():
    root, src, dst, layout_name, qf_raw, variant, seed = sys.argv[1:8]
    probe = [int(v) for v in sys.argv[8:]]
    params, offsets = gen_dequant.load_tables(root)
    data = open(src, "rb").read()
    out = rebuild(data, probe, acswriter.LAYOUTS[layout_name], int(qf_raw), variant, params, offsets, int(seed))
    open(dst, "wb").write(out)


if __name__ == "__main__":
    main()
