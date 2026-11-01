import base64
import os
import re
import subprocess
import sys
import tempfile
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
BASE = "https://aomedia.googlesource.com/aom/+/refs/heads/main/"
WL = [2, 3, 4, 5, 6, 2, 3, 3, 4, 4, 5, 5, 6, 2, 4, 3, 5, 4, 6]
HL = [2, 3, 4, 5, 6, 3, 2, 4, 3, 5, 4, 6, 5, 4, 2, 5, 3, 6, 4]
VT = [0, 1, 0, 1, 2, 0, 2, 1, 2, 3, 0, 3, 1, 3, 2, 3]
HT = [0, 0, 1, 1, 0, 2, 2, 2, 1, 3, 3, 0, 3, 1, 3, 2]
BDS = [8, 10, 12]

SHIM = """#ifndef SHIM_TXFM_H
#define SHIM_TXFM_H
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#define cos_bit_min 10
#define DO_RANGE_CHECK_CLAMP 0
extern const int32_t av1_cospi_arr_data[4][64];
extern const int32_t av1_sinpi_arr_data[4][5];
#define NewSqrt2Bits ((int32_t)12)
static const int32_t NewSqrt2 = 5793;
static const int32_t NewInvSqrt2 = 2896;
static inline const int32_t *cospi_arr(int n) { return av1_cospi_arr_data[n - cos_bit_min]; }
static inline const int32_t *sinpi_arr(int n) { return av1_sinpi_arr_data[n - cos_bit_min]; }
static inline int64_t clamp64(int64_t v, int64_t lo, int64_t hi) { return v < lo ? lo : (v > hi ? hi : v); }
static inline int64_t range_check_value64(int64_t value, int8_t bit) { (void)bit; return value; }
static inline void av1_range_check_buf(int32_t s, const int32_t *i, const int32_t *b, int32_t n, int8_t bit) { (void)s; (void)i; (void)b; (void)n; (void)bit; }
static inline int32_t round_shift(int64_t value, int bit) { return (int32_t)((value + (1ll << (bit - 1))) >> bit); }
static inline int32_t half_btf(int32_t w0, int32_t in0, int32_t w1, int32_t in1, int bit) {
  int64_t result_64 = (int64_t)(w0 * in0) + (int64_t)(w1 * in1);
  int64_t intermediate = result_64 + (1LL << (bit - 1));
  return (int32_t)(intermediate >> bit);
}
#endif
"""


def fetch(path, dst):
    data = urllib.request.urlopen(BASE + path + "?format=TEXT").read()
    open(dst, "wb").write(base64.b64decode(data))


def tables():
    text = open(os.path.join(ROOT, "lib/media/video/av1/av1tabtx.fi")).read()
    cos = [int(x) for x in re.findall(r"-?\d+", text.split("COSPI_ARR: [i32; 256] = [")[1].split("]")[0])]
    sin = [int(x) for x in re.findall(r"-?\d+", text.split("SINPI_ARR: [i32; 20] = [")[1].split("]")[0])]
    return cos, sin


def build_itx_oracle(d, src=None):
    inc = os.path.join(d, "inc/av1/common")
    os.makedirs(inc)
    for f in ("av1_inv_txfm1d.c", "av1_inv_txfm1d.h"):
        if src:
            open(os.path.join(inc, f), "wb").write(open(os.path.join(src, f), "rb").read())
        else:
            fetch("av1/common/" + f, os.path.join(inc, f))
    open(os.path.join(inc, "av1_txfm.h"), "w").write(SHIM)
    cos, sin = tables()
    with open(os.path.join(d, "tabs.c"), "w") as f:
        f.write("#include <stdint.h>\n")
        f.write("const int32_t av1_cospi_arr_data[4][64] = {%s};\n" % ",".join("{%s}" % ",".join(map(str, cos[i * 64:i * 64 + 64])) for i in range(4)))
        f.write("const int32_t av1_sinpi_arr_data[4][5] = {%s};\n" % ",".join("{%s}" % ",".join(map(str, sin[i * 5:i * 5 + 5])) for i in range(4)))
    exe = os.path.join(d, "itx_oracle")
    subprocess.check_call(["gcc", "-O1", "-fwrapv", "-I", os.path.join(d, "inc"), "-o", exe,
                           os.path.join(HERE, "av1_itx_oracle.c"), os.path.join(inc, "av1_inv_txfm1d.c"),
                           os.path.join(d, "tabs.c"), "-lm"])
    return exe


class Rng:
    def __init__(self, seed):
        self.s = seed

    def rnd(self, n):
        self.s = (self.s * 1103515245 + 12345) & 2147483647
        return ((self.s >> 16) & 32767) % n

    def big(self, n):
        return (self.rnd(32768) * 32768 + self.rnd(32768)) % n


def fnv(h, b):
    return ((h ^ b) * 16777619) & 4294967295


def kind_valid(kind, lg):
    if kind == 0:
        return True
    if kind == 3:
        return lg <= 5
    return lg <= 4


def valid_types(ts):
    return [t for t in range(16) if kind_valid(VT[t], HL[ts]) and kind_valid(HT[t], WL[ts])]


def itx_case(seed, w, h, bd, lossless=False):
    r = Rng(seed)
    coefs = [0] * (w * h)
    cw, ch = min(w, 32), min(h, 32)
    cls = r.rnd(3)
    if lossless:
        cls = 0
    mag = [200, 1 << (bd + 3), 1 << (bd + 6)][cls]
    if r.rnd(4) == 0:
        for y in range(ch):
            for x in range(cw):
                coefs[y * w + x] = r.big(2 * 40 + 1) - 40
    else:
        for _ in range(1 + r.rnd(12)):
            x = r.rnd(cw)
            y = r.rnd(ch)
            coefs[y * w + x] = r.big(2 * mag + 1) - mag
    dest = [r.big(1 << bd) for _ in range(w * h)]
    return coefs, dest


def itx_cases():
    out = []
    for ts in range(19):
        for tt in valid_types(ts):
            for bd in BDS:
                for k in range(2):
                    out.append((ts, tt, bd, ((ts * 16 + tt) * 3 + BDS.index(bd)) * 4 + k + 1))
    for bd in BDS:
        for k in range(8):
            out.append((-1, 0, bd, 100000 + bd * 10 + k))
    return out


def run_itx_expect(exe, cases):
    lines = []
    for ts, tt, bd, seed in cases:
        w, h = (4, 4) if ts < 0 else (1 << WL[ts], 1 << HL[ts])
        coefs, dest = itx_case(seed, w, h, bd, ts < 0)
        lines.append("%d %d %d\n%s\n%s\n" % (ts, tt, bd, " ".join(map(str, coefs)), " ".join(map(str, dest))))
    res = subprocess.run([exe], input="".join(lines), capture_output=True, text=True, check=True).stdout.strip().split("\n")
    out = []
    for line in res:
        h = 2166136261
        for v in line.split():
            h = fnv(h, int(v))
        out.append(h)
    return out


def fmt_array(vals, per=8):
    rows = []
    for i in range(0, len(vals), per):
        rows.append("    " + ", ".join(str(v) for v in vals[i:i + per]) + ",")
    return "\n".join(rows)


def main():
    cmd = sys.argv[1]
    src = sys.argv[sys.argv.index("--src") + 1] if "--src" in sys.argv else None
    outdir = sys.argv[2]
    with tempfile.TemporaryDirectory() as d:
        if cmd == "itx":
            exe = build_itx_oracle(d, src)
            cases = itx_cases()
            exp = run_itx_expect(exe, cases)
            tpl = open(os.path.join(HERE, "av1_itx_test.tpl")).read()
            tpl = tpl.replace("@COUNT@", str(len(exp))).replace("@EXPECT@", fmt_array(exp))
            open(os.path.join(outdir, "6590_av1_itx.fi"), "w").write(tpl)
            print("itx cases", len(exp))


if __name__ == "__main__":
    main()
