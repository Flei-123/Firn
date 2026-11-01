import base64
import os
import re
import subprocess
import sys
import tempfile
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from av1_recon_ref import BDS, Rng, fmt_array

SHA = "d55e6b33a8dea2fbe6b16acc31918e15f2a93602"
BASE = "https://aomedia.googlesource.com/aom/+/" + SHA + "/"
FILES = [
    "aom_dsp/blend_a64_mask.c",
    "aom_dsp/blend_a64_hmask.c",
    "aom_dsp/blend_a64_vmask.c",
    "av1/common/convolve.c",
    "av1/common/filter.h",
    "av1/common/reconinter.c",
    "av1/common/warped_motion.c",
    "av1/common/mvref_common.c",
    "tools/gen_wedge_masks_data.py",
]
STUBS = ["config/aom_config.h", "config/aom_dsp_rtcd.h", "aom/aom_integer.h", "aom_ports/mem.h", "aom_dsp/aom_dsp_common.h", "aom_dsp/blend.h"]
NPARAMS = [21, 2, 11, 6, 25, 39]
BLK = [(4, 4), (4, 8), (8, 4), (8, 8), (8, 16), (16, 8), (16, 16), (16, 32), (32, 16), (32, 32), (32, 64), (64, 32), (64, 64), (64, 128), (128, 64), (128, 128), (4, 16), (16, 4), (8, 32), (32, 8), (16, 64), (64, 16)]
WEDGE_BS = [3, 4, 5, 6, 7, 8, 9, 18, 19]
SMOOTH_BS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 16, 17, 18, 19]
DIMS = [2, 4, 8, 16, 32, 64, 128]
WARP_BLK = [b for b in BLK if min(b) >= 8]
PAIRS = [(9, 7), (7, 9), (11, 5), (5, 11), (12, 4), (4, 12), (13, 3), (3, 13)]


def fetch(path, dst, src):
    if src:
        open(dst, "wb").write(open(os.path.join(src, os.path.basename(path)), "rb").read())
        return
    data = urllib.request.urlopen(BASE + path + "?format=TEXT").read()
    open(dst, "wb").write(base64.b64decode(data))


def lines_of(path):
    return open(path).read().split("\n")


def find(lines, pattern, start=0):
    return next(i for i in range(start, len(lines)) if re.match(pattern, lines[i]))


def func_at(lines, pattern):
    s = find(lines, pattern)
    e = find(lines, r"^}", s)
    return lines[s:e + 1]


def filter_segment(lines):
    s = find(lines, r"^#define MAX_FILTER_TAP")
    a = find(lines, r"^static const uint16_t$")
    b = find(lines, r"^static const InterpFilterParams av1_interp_4tap")
    c = find(lines, r"^static inline const InterpFilterParams \*av1_get_filter\(")
    return lines[s:a] + lines[b:c]


def recon_segments(lines):
    out = []
    out += func_at(lines, r"^static inline void diffwtd_mask_d16")
    out += func_at(lines, r"^void av1_build_compound_diffwtd_mask_d16_c")
    s = find(lines, r"^static const uint8_t obmc_mask_1")
    e = find(lines, r"^}", find(lines, r"^const uint8_t \*av1_get_obmc_mask"))
    return out + lines[s:e + 1]


def warp_segments(lines):
    s = find(lines, r"^const WarpedFilterCoeff av1_warped_filter")
    e = find(lines, r"^void highbd_warp_plane")
    first = [l for l in lines[s:e] if not l.startswith("#if CONFIG_AV1_HIGHBITDEPTH")]
    t = find(lines, r"^#define LS_MV_MAX")
    return first + lines[t:]


def samp_segments(lines):
    out = func_at(lines, r"^static inline void record_samples")
    out += func_at(lines, r"^uint8_t av1_selectSamples")
    out += func_at(lines, r"^uint8_t av1_findSamples")
    return out


def write_wedge_bin(d, work):
    sys.path.insert(0, work)
    import gen_wedge_masks_data as gw
    obl = gw.build_wedge_mask_obl()
    buf, offsets = gw.build_wedge_mask_buf(obl)
    used = 0
    for bs in range(gw.BLOCK_SIZES_ALL):
        if gw.WEDGE_PARAMS_LOOKUP[bs][0] > 0:
            used += 32 * gw.BLOCK_WIDTH[bs] * gw.BLOCK_HEIGHT[bs]
    out = bytearray(buf[:used])
    smooth = gw.build_smooth_interintra_mask_buf()
    for m in range(gw.INTERINTRA_MODES):
        for bs in range(gw.BLOCK_SIZES_ALL):
            bw, bh = gw.BLOCK_WIDTH[bs], gw.BLOCK_HEIGHT[bs]
            if bw <= 32 and bh <= 32:
                out += bytes(smooth[m][bs][:bw * bh])
    path = os.path.join(d, "wedge.bin")
    open(path, "wb").write(out)
    return path


def build_oracle(d, src=None):
    inc = os.path.join(d, "inc")
    for sub in ("config", "aom_dsp", "aom_ports", "aom"):
        os.makedirs(os.path.join(inc, sub))
    for f in FILES:
        fetch(f, os.path.join(d, os.path.basename(f)), src)
    shim = open(os.path.join(HERE, "av1_inter_shim.h")).read()
    open(os.path.join(inc, "av1_inter_shim.h"), "w").write(shim)
    for stub in STUBS:
        open(os.path.join(inc, stub), "w").write('#include "av1_inter_shim.h"\n')
    conv = lines_of(os.path.join(d, "convolve.c"))
    filt = lines_of(os.path.join(d, "filter.h"))
    recon = lines_of(os.path.join(d, "reconinter.c"))
    warp = lines_of(os.path.join(d, "warped_motion.c"))
    out = ['#include "av1_inter_shim.h"', "typedef int BLOCK_SIZE;"]
    out.append("static const int block_size_wide[22] = {%s};" % ",".join(str(w) for w, h in BLK))
    out.append("static const int block_size_high[22] = {%s};" % ",".join(str(h) for w, h in BLK))
    out += filter_segment(filt) + [""]
    out += func_at(conv, r"^void av1_highbd_convolve_2d_scale_c") + [""]
    out += recon_segments(recon) + [""]
    out += warp_segments(warp) + [""]
    mvref = lines_of(os.path.join(d, "mvref_common.c"))
    out.append(open(os.path.join(HERE, "av1_inter_samp_defs.h")).read())
    out.append("static const int mi_size_wide[22] = {%s};" % ",".join(str(w // 4) for w, h in BLK))
    out.append("static const int mi_size_high[22] = {%s};" % ",".join(str(h // 4) for w, h in BLK))
    out += samp_segments(mvref) + [""]
    for f in ("blend_a64_mask.c", "blend_a64_hmask.c", "blend_a64_vmask.c"):
        out.append('#include "%s"' % f)
    for f in ("av1_inter_oracle_mc.c", "av1_inter_oracle_more.c", "av1_inter_oracle_samp.c", "av1_inter_oracle_main.c"):
        out.append(open(os.path.join(HERE, f)).read())
    open(os.path.join(d, "inter_tu.c"), "w").write("\n".join(out))
    exe = os.path.join(d, "inter_oracle")
    subprocess.check_call(["gcc", "-O1", "-fwrapv", "-DNDEBUG", "-w", "-I", inc, "-I", d, "-o", exe, os.path.join(d, "inter_tu.c"), "-lm"])
    return exe, write_wedge_bin(d, d)


def gen_mc(r):
    rows = []
    n = 0
    for bd in BDS:
        for w in DIMS:
            for h in DIMS:
                for rep in range(2):
                    n += 1
                    fw = 16 + r.rnd(113)
                    fh = 16 + r.rnd(113)
                    cls = r.rnd(5)
                    rw, rh = fw, fh
                    if cls >= 1:
                        lo_w, lo_h = (fw + 15) // 16, (fh + 15) // 16
                        if cls == 1:
                            rw, rh = 2 * fw, 2 * fh
                        elif cls == 2:
                            rw, rh = lo_w, lo_h
                        else:
                            rw = lo_w + r.rnd(2 * fw - lo_w + 1)
                            rh = lo_h + r.rnd(2 * fh - lo_h + 1)
                    x = r.rnd(fw + 1)
                    y = r.rnd(fh + 1)
                    big = r.rnd(4) != 0
                    span = 1025 if big else 33
                    half = span // 2
                    mv = [r.rnd(span) - half for _ in range(4)]
                    mode = r.rnd(3)
                    fwd, bck = PAIRS[r.rnd(8)]
                    rows.append([0, bd, w, h, r.rnd(4), r.rnd(4), rw, rh, fw, fh, x, y, r.rnd(2), r.rnd(2),
                                 mv[0], mv[1], mv[2], mv[3], mode, fwd, bck, n * 7919 + 13])
    return rows


def gen_dist():
    rows = []
    for d0 in range(17):
        for d1 in range(17):
            rows.append([1, d0, d1])
    for d0 in (31, 40, 64):
        for d1 in (1, 2, 5, 31, 63):
            rows.append([1, d0, d1])
            rows.append([1, d1, d0])
    return rows


def gen_mask(r):
    rows = []
    n = 0
    sss = [(0, 0), (1, 1), (1, 0), (0, 1)]
    for bd in BDS:
        for bs in WEDGE_BS:
            for ssx, ssy in sss:
                for typ in (0, 3):
                    n += 1
                    bw, bh = BLK[bs]
                    sg = r.rnd(2) if typ == 0 else 0
                    rows.append([2, bd, bw >> ssx, bh >> ssy, ssx, ssy, typ, bs, r.rnd(16), sg, 0, n * 104729 + 7])
        for lw in (8, 16, 32, 64, 128):
            for lh in (8, 16, 32, 64, 128):
                for ssx, ssy in sss:
                    n += 1
                    rows.append([2, bd, lw >> ssx, lh >> ssy, ssx, ssy, 1, 0, 0, 0, r.rnd(2), n * 104729 + 7])
        for bs in SMOOTH_BS:
            for mode in range(4):
                n += 1
                bw, bh = BLK[bs]
                rows.append([2, bd, bw, bh, 0, 0, 2, bs, 0, mode, 0, n * 104729 + 7])
    return rows


def gen_obmc(r):
    rows = []
    n = 0
    for bd in BDS:
        for ov in (1, 2, 4, 8, 16, 32, 64):
            for d in (0, 1):
                for rep in range(3):
                    n += 1
                    ok = [v for v in DIMS if v >= ov]
                    w = DIMS[r.rnd(len(DIMS))] if d == 0 else ok[r.rnd(len(ok))]
                    h = ok[r.rnd(len(ok))] if d == 0 else DIMS[r.rnd(len(DIMS))]
                    if w * h > 4096:
                        w, h = min(w, 64), min(h, 64)
                        if d == 0:
                            h = max(h, ov)
                        else:
                            w = max(w, ov)
                    rows.append([3, bd, w, h, ov, d, n * 15485863 + 11])
    return rows


def rand_mat(r):
    style = r.rnd(10)
    t0 = (r.rnd(41) - 20) * 65536 + r.rnd(65536)
    t1 = (r.rnd(41) - 20) * 65536 + r.rnd(65536)
    if style <= 7:
        a = 65536 + r.rnd(10001) - 5000
        d = 65536 + r.rnd(10001) - 5000
        b = r.rnd(5001) - 2500
        c = r.rnd(5001) - 2500
    elif style == 8:
        a = 65536 + r.rnd(40001) - 20000
        d = 65536 + r.rnd(40001) - 20000
        b = r.rnd(40001) - 20000
        c = r.rnd(40001) - 20000
    else:
        a = r.rnd(200000) - 30000
        d = r.rnd(200000) - 30000
        b = r.rnd(20001) - 10000
        c = r.rnd(20001) - 10000
    return [t0, t1, a, b, c, d]


def gen_warp(r):
    rows = []
    n = 0
    for bd in BDS:
        for w in (4, 8, 16, 32, 64):
            for h in (4, 8, 16, 32, 64):
                for rep in range(3):
                    n += 1
                    rw = 16 + r.rnd(145)
                    rh = 16 + r.rnd(145)
                    fwd, bck = PAIRS[r.rnd(8)]
                    rows.append([4, bd, w, h, r.rnd(rw), r.rnd(rh), r.rnd(2), r.rnd(2), rw, rh, r.rnd(3), fwd, bck,
                                 n * 1299709 + 3] + rand_mat(r) + rand_mat(r))
    return rows


def gen_warpest(r):
    rows = []
    for bw, bh in WARP_BLK:
        for rep in range(8):
            np_ = 1 + r.rnd(8)
            mvy = r.rnd(1025) - 512
            mvx = r.rnd(1025) - 512
            p1 = []
            p2 = []
            for i in range(16):
                x = (r.rnd(bw + 129) - 64) * 8 + r.rnd(8)
                y = (r.rnd(bh + 129) - 64) * 8 + r.rnd(8)
                spread = (41, 41, 41, 101, 401, 1501)[r.rnd(6)]
                p1 += [x, y]
                p2 += [x + mvx + r.rnd(spread) - spread // 2, y + mvy + r.rnd(spread) - spread // 2]
            rows.append([5, np_, bw, bh, mvy, mvx, r.rnd(300), r.rnd(300)] + p1[:16] + p2[:16])
    return rows


def gen_samples(r):
    rows = []
    for rep in range(40):
        for cur in range(22):
            if rep >= 14 and max(BLK[cur]) < 128:
                continue
            bw4 = BLK[cur][0] // 4
            bh4 = BLK[cur][1] // 4
            if bw4 > 32 or bh4 > 32:
                continue
            mi_row = r.rnd(300)
            mi_col = r.rnd(300)
            if r.rnd(10) < 7:
                mi_col = mi_col // bw4 * bw4
                mi_row = mi_row // bh4 * bh4
            mi_rows = mi_row + 1 + r.rnd(bh4 * 2)
            mi_cols = mi_col + 1 + r.rnd(bw4 * 2)
            up = 1 if mi_row > 0 and r.rnd(10) < 9 else 0
            left = 1 if mi_col > 0 and r.rnd(10) < 9 else 0
            tr = r.rnd(2)
            ref0 = 1 + r.rnd(7)
            mvr = r.rnd(401) - 200
            mvc = r.rnd(401) - 200
            row = [6, mi_row, mi_col, bw4, bh4, mi_rows, mi_cols, up, left, tr, ref0, mvr, mvc, cur]
            hit = (2, 3, 3, 4)[r.rnd(4)]
            spread = (17, 61, 241)[r.rnd(3)]
            for i in range(bw4 + 2 + bh4):
                rf0 = ref0 if r.rnd(4) < hit else r.rnd(8)
                rf1 = -1 if r.rnd(8) else 1 + r.rnd(7)
                row += [rf0, rf1, r.rnd(22), mvr + r.rnd(spread) - spread // 2, mvc + r.rnd(spread) - spread // 2]
            rows.append(row)
    return rows


def all_cases():
    r = Rng(20261101)
    rows = gen_mc(r) + gen_dist() + gen_mask(r) + gen_obmc(r) + gen_warp(r)
    rows += gen_warpest(r)
    return rows


def row_text(row):
    return " ".join(str(v) for v in row)


def run_cases(exe, wedge, rows):
    text = "\n".join(row_text(r) for r in rows) + "\n"
    res = subprocess.run([exe, wedge], input=text, capture_output=True, text=True, check=True).stdout.split()
    return [int(v) for v in res]


def write_samp_test(outdir, rows, exp):
    assert len(exp) == len(rows)
    flat = [v for row in rows for v in row[1:]]
    tpl = open(os.path.join(HERE, "av1_samp_test.tpl")).read()
    tpl = tpl.replace("@CASES@", str(len(rows))).replace("@FLAT_COUNT@", str(len(flat)))
    tpl = tpl.replace("@FLAT@", fmt_array(flat, 16)).replace("@EXPECT@", fmt_array(exp))
    tpl = tpl.replace("@BW4@", ", ".join(str(w // 4) for w, h in BLK)).replace("@BH4@", ", ".join(str(h // 4) for w, h in BLK))
    open(os.path.join(outdir, "6611_av1_warp_samples.fi"), "w").write(tpl)
    print("samples cases", len(rows))


def main():
    outdir = sys.argv[1]
    src = sys.argv[sys.argv.index("--src") + 1] if "--src" in sys.argv else None
    with tempfile.TemporaryDirectory() as d:
        exe, wedge = build_oracle(d, src)
        rows = all_cases()
        if "--only" in sys.argv:
            only = int(sys.argv[sys.argv.index("--only") + 1])
            rows = [r for r in rows if r[0] == only]
        exp = run_cases(exe, wedge, rows)
        samp_rows = gen_samples(Rng(20261102))
        samp_exp = run_cases(exe, wedge, samp_rows)
    assert len(exp) == len(rows)
    write_samp_test(outdir, samp_rows, samp_exp)
    flat = [v for row in rows for v in row]
    tpl = open(os.path.join(HERE, "av1_inter_test.tpl")).read()
    tpl = tpl.replace("@CASES@", str(len(rows))).replace("@FLAT_COUNT@", str(len(flat)))
    tpl = tpl.replace("@FLAT@", fmt_array(flat, 16)).replace("@EXPECT@", fmt_array(exp))
    open(os.path.join(outdir, "6610_av1_inter_pred.fi"), "w").write(tpl)
    print("inter cases", len(rows))


if __name__ == "__main__":
    main()
