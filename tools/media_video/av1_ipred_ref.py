import base64
import os
import re
import subprocess
import sys
import tempfile
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from av1_recon_ref import BDS, Rng, fmt_array, fnv

SHA = "d55e6b33a8dea2fbe6b16acc31918e15f2a93602"
BASE = "https://aomedia.googlesource.com/aom/+/" + SHA + "/"
FILES = [
    "aom_dsp/intrapred.c",
    "aom_dsp/intrapred_common.h",
    "av1/common/reconintra.c",
    "av1/common/reconintra.h",
    "av1/common/cfl.c",
    "av1/common/cfl.h",
]
WL = [2, 3, 4, 5, 6, 2, 3, 3, 4, 4, 5, 5, 6, 2, 4, 3, 5, 4, 6]
HL = [2, 3, 4, 5, 6, 3, 2, 4, 3, 5, 4, 6, 5, 4, 2, 5, 3, 6, 4]
PW = 144
PH = 144
SLOTS = 18
SUBS = [(1, 1), (1, 0), (0, 0)]

RECON_SEGMENTS = [
    (r"^enum \{", r"^#define NUM_INTRA_NEIGHBOUR_PIXELS", True),
    (r"^static const uint8_t extend_modes", r"^};", True),
    (r"^static intra_pred_fn pred\[", r"^#endif", True),
    (r"^static void init_intra_predictors_internal", r"^}", True),
    (r"^void av1_highbd_dr_prediction_z1_c", r"^#endif  // CONFIG_AV1_HIGHBITDEPTH", False),
    (r"^DECLARE_ALIGNED\(16, const int8_t,", r"^};", True),
    (r"^static void highbd_filter_intra_predictor", r"^}", True),
    (r"^static int intra_edge_filter_strength", r"^}", True),
    (r"^void av1_highbd_filter_intra_edge_c", r"^#endif  // CONFIG_AV1_HIGHBITDEPTH", False),
]
CFL_SEGMENTS = [
    (r"^static inline void cfl_pad", r"^}", True),
    (r"^static void subtract_average_c", r"^}", True),
    (r"^static inline void cfl_predict_hbd_c", r"^}", True),
    (r"^static void cfl_luma_subsampling_420_hbd_c", r"^}", True),
    (r"^static void cfl_luma_subsampling_422_hbd_c", r"^}", True),
    (r"^static void cfl_luma_subsampling_444_hbd_c", r"^}", True),
]


def fetch(path, dst):
    data = urllib.request.urlopen(BASE + path + "?format=TEXT").read()
    open(dst, "wb").write(base64.b64decode(data))


def grab(lines, start, end, include_end):
    s = next(i for i, l in enumerate(lines) if re.match(start, l))
    e = next(i for i in range(s, len(lines)) if re.match(end, lines[i]))
    return lines[s:e + 1] if include_end else lines[s:e]


def read_lines(path):
    return open(path).read().split("\n")


def header_chunk(lines):
    s = next(i for i, l in enumerate(lines) if l.startswith("static const int16_t dr_intra_derivative"))
    e = max(i for i, l in enumerate(lines) if l.startswith("#ifdef __cplusplus"))
    return lines[s:e]


def build_oracle(d, src=None):
    inc = os.path.join(d, "inc")
    for sub in ("config", "aom_dsp", "aom_mem", "aom_ports", "av1/common"):
        os.makedirs(os.path.join(inc, sub))
    for f in FILES:
        dst = os.path.join(d, os.path.basename(f))
        if src:
            open(dst, "wb").write(open(os.path.join(src, os.path.basename(f)), "rb").read())
        else:
            fetch(f, dst)
    shim = open(os.path.join(HERE, "av1_ipred_shim.h")).read()
    open(os.path.join(inc, "av1_ipred_shim.h"), "w").write(shim)
    for stub in ("config/aom_config.h", "config/aom_dsp_rtcd.h", "aom_dsp/aom_dsp_common.h", "aom_mem/aom_mem.h", "aom_ports/bitops.h"):
        open(os.path.join(inc, stub), "w").write('#include "av1_ipred_shim.h"\n')
    open(os.path.join(inc, "aom_dsp/intrapred_common.h"), "wb").write(open(os.path.join(d, "intrapred_common.h"), "rb").read())
    recon = read_lines(os.path.join(d, "reconintra.c"))
    cfl = read_lines(os.path.join(d, "cfl.c"))
    out = ['#include "av1_ipred_shim.h"', '#include "intrapred.c"']
    out += header_chunk(read_lines(os.path.join(d, "reconintra.h"))) + [""]
    for start, end, inc_end in RECON_SEGMENTS:
        seg = grab(recon, start, end, inc_end)
        seg = [re.sub(r"(aom_##type##_predictor_\d+x\d+);", r"\1_c;", l) for l in seg]
        out += seg + [""]
    out += grab(read_lines(os.path.join(d, "cfl.h")), r"^static inline int get_scaled_luma_q0", r"^}", True) + [""]
    for start, end, inc_end in CFL_SEGMENTS:
        out += grab(cfl, start, end, inc_end) + [""]
    out.append(open(os.path.join(HERE, "av1_ipred_oracle.c")).read())
    open(os.path.join(d, "ipred_tu.c"), "w").write("\n".join(out))
    exe = os.path.join(d, "ipred_oracle")
    subprocess.check_call(["gcc", "-O1", "-fwrapv", "-w", "-I", inc, "-I", d, "-o", exe, os.path.join(d, "ipred_tu.c"), "-lm"])
    return exe


def pred_params(seed, w, h):
    r = Rng(seed)
    hl = r.rnd(5) != 0
    ha = r.rnd(5) != 0
    t1 = r.rnd(2)
    t2 = r.rnd(2)
    htr = ha and t1 == 1
    hbl = hl and t2 == 1
    cx = r.rnd(8)
    rx = r.rnd(32)
    ry = r.rnd(32)
    sx = r.rnd(2 * w - 1)
    sy = r.rnd(2 * h - 1)
    cy = r.rnd(8)
    xr = edge_dist(cx, w, rx, sx)
    yd = edge_dist(cy, h, ry, sy)
    et = r.rnd(2)
    ef = r.rnd(5) != 0
    x = 1 + r.rnd(8)
    y = 1 + r.rnd(8)
    return hl, ha, htr, hbl, xr, yd, et, ef, x, y


def edge_dist(c, n, big, mid):
    if c < 4:
        return n + big
    if c < 6:
        return mid - (n - 1)
    if c == 6:
        return 0
    return 1 + (big % 4)


def cfl_params(seed, w, h):
    r = Rng(seed)
    alpha = r.rnd(33) - 16
    c1 = r.rnd(3)
    r1 = r.rnd(w)
    c2 = r.rnd(3)
    r2 = r.rnd(h)
    vw = w if c1 == 0 else 1 + r1
    vh = h if c2 == 0 else 1 + r2
    lx = 1 + r.rnd(8)
    ly = 1 + r.rnd(8)
    return alpha, vw, vh, lx, ly


def plane_seed(seed):
    return seed * 7919 + 13


def enumerate_cases():
    cases = []
    n = 0
    for ts in range(19):
        w, h = 1 << WL[ts], 1 << HL[ts]
        for bd in BDS:
            for slot in range(SLOTS):
                if slot >= 13 and (w > 32 or h > 32):
                    continue
                deltas = range(-3, 4) if 1 <= slot <= 8 else [0]
                for ad in deltas:
                    for k in range(2):
                        n += 1
                        cases.append(("p", ts, bd, slot, ad, n))
    for ts in range(19):
        if WL[ts] > 5 or HL[ts] > 5:
            continue
        for bd in BDS:
            for si in range(3):
                for k in range(3):
                    n += 1
                    cases.append(("c", ts, bd, si, 0, n))
    return cases


def case_line(c):
    kind, ts, bd, a, ad, seed = c
    w, h = 1 << WL[ts], 1 << HL[ts]
    if kind == "p":
        mode, fm = (a, -1) if a <= 12 else (0, a - 13)
        hl, ha, htr, hbl, xr, yd, et, ef, x, y = pred_params(seed, w, h)
        return "0 %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d" % (
            bd, ts, mode, ad, fm, hl, ha, htr, hbl, xr, yd, et, ef, x, y, plane_seed(seed))
    sx, sy = SUBS[a]
    alpha, vw, vh, lx, ly = cfl_params(seed, w, h)
    return "1 %d %d %d %d %d %d %d %d %d %d" % (bd, ts, sx, sy, alpha, vw, vh, lx, ly, plane_seed(seed))


def run_cases(exe, cases):
    text = "\n".join(case_line(c) for c in cases) + "\n"
    res = subprocess.run([exe], input=text, capture_output=True, text=True, check=True).stdout.split()
    return [int(v) for v in res]


def main():
    outdir = sys.argv[1]
    src = sys.argv[sys.argv.index("--src") + 1] if "--src" in sys.argv else None
    with tempfile.TemporaryDirectory() as d:
        exe = build_oracle(d, src)
        cases = enumerate_cases()
        exp = run_cases(exe, cases)
    tpl = open(os.path.join(HERE, "av1_ipred_test.tpl")).read()
    tpl = tpl.replace("@COUNT@", str(len(exp))).replace("@EXPECT@", fmt_array(exp))
    open(os.path.join(outdir, "6591_av1_ipred.fi"), "w").write(tpl)
    print("ipred cases", len(exp))


if __name__ == "__main__":
    main()
