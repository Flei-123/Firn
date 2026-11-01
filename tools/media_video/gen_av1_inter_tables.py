import base64
import os
import re
import sys
import urllib.request

SHA = "d55e6b33a8dea2fbe6b16acc31918e15f2a93602"
BASE = "https://aomedia.googlesource.com/aom/+/" + SHA + "/"
FILES = ["av1/common/warped_motion.c", "av1/common/reconinter.c", "tools/gen_wedge_masks_data.py"]


def fetch_all(dst, src):
    os.makedirs(dst, exist_ok=True)
    for f in FILES:
        p = os.path.join(dst, os.path.basename(f))
        if src:
            open(p, "wb").write(open(os.path.join(src, os.path.basename(f)), "rb").read())
        else:
            open(p, "wb").write(base64.b64decode(urllib.request.urlopen(BASE + f + "?format=TEXT").read()))


def numbers(text, start):
    i = re.search(start, text).end()
    j = text.index("};", i)
    body = re.sub(r"//[^\n]*", "", text[i:j])
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
    body = re.sub(r"-\s+(\d)", r"-\1", body)
    return [int(v) for v in re.findall(r"-?\d+", body)]


def emit(name, vals, out):
    rows = []
    for k in range(0, len(vals), 16):
        rows.append("    " + ", ".join(str(v) for v in vals[k:k + 16]))
    out.append("static %s: [i16; %d] = [\n%s\n]\n" % (name.upper(), len(vals), ",\n".join(rows)))
    out.append("fn %s_addr() -> u64 {\n    return (&%s[0]) as u64\n}\n" % (name.lower(), name.upper()))


def main():
    outdir = sys.argv[1]
    src = sys.argv[sys.argv.index("--src") + 1] if "--src" in sys.argv else None
    work = os.path.join(outdir, ".gen_src")
    fetch_all(work, src)
    sys.path.insert(0, work)
    import gen_wedge_masks_data as gw
    warp = open(os.path.join(work, "warped_motion.c")).read()
    recon = open(os.path.join(work, "reconinter.c")).read()
    wf = numbers(warp, r"av1_warped_filter\[WARPEDPIXEL_PREC_SHIFTS \* 3 \+ 1\]\s*\[8\] = \{")
    dl = numbers(warp, r"static const uint16_t div_lut\[DIV_LUT_NUM \+ 1\] = \{")
    assert len(wf) == 193 * 8 and len(dl) == 257
    obmc = []
    for n in (1, 2, 4, 8, 16, 32, 64):
        v = numbers(recon, r"obmc_mask_%d\[%d\]\)? = \{" % (n, n))
        assert len(v) == n
        obmc += v
    qw = [2, 3, 2, 5, 2, 7, 1, 31]
    ql = [9, 7, 11, 5, 12, 4, 13, 3]
    books = [gw.WEDGE_CODEBOOK_16_HEQW, gw.WEDGE_CODEBOOK_16_HGTW, gw.WEDGE_CODEBOOK_16_HLTW]
    cb = [v for b in books for e in b for v in e]
    sf = [v for r in gw.WEDGE_SIGNFLIP_LOOKUP for v in r]
    bk = [0 if gw.WEDGE_PARAMS_LOOKUP[i][1] is None else [id(b) for b in books].index(id(gw.WEDGE_PARAMS_LOOKUP[i][1])) for i in range(22)]
    hdr = "// SPDX-License-Identifier: MPL-2.0\n"
    t = [hdr]
    mask_names = ["wedge_master_odd", "wedge_master_even", "wedge_master_vert", "wedge_signflip", "wedge_codebook", "wedge_book_of", "ii_weights", "ii_scales", "blk_w", "blk_h", "obmc_masks", "dist_weight", "dist_lookup"]
    t.append("export { %s }\n" % ", ".join(n + "_addr" for n in mask_names))
    emit("wedge_master_odd", gw.WEDGE_MASTER_OBLIQUE_ODD, t)
    emit("wedge_master_even", gw.WEDGE_MASTER_OBLIQUE_EVEN, t)
    emit("wedge_master_vert", gw.WEDGE_MASTER_VERTICAL, t)
    emit("wedge_signflip", sf, t)
    emit("wedge_codebook", cb, t)
    emit("wedge_book_of", bk, t)
    emit("ii_weights", gw.II_WEIGHTS_1D, t)
    emit("ii_scales", gw.II_SIZE_SCALES, t)
    emit("blk_w", gw.BLOCK_WIDTH, t)
    emit("blk_h", gw.BLOCK_HEIGHT, t)
    emit("obmc_masks", obmc, t)
    emit("dist_weight", qw, t)
    emit("dist_lookup", ql, t)
    open(os.path.join(outdir, "av1tabmask.fi"), "w").write("\n".join(t))
    w = [hdr, "export { warped_filter_addr, div_lut_addr }\n"]
    emit("warped_filter", wf, w)
    emit("div_lut", dl, w)
    open(os.path.join(outdir, "av1tabwarp.fi"), "w").write("\n".join(w))
    print("tables ok")


if __name__ == "__main__":
    main()
