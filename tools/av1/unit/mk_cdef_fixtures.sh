#!/bin/bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/av1/mk_cdef_fixtures.sh <outdir> -- CDEF fixtures: libaom 4:4:4 stills with CDEF on, restoration + palette off, 64x64 superblocks,
# source = lavfi gradients + noise (NOT testsrc2).  cdef_<n>.avif = encoder-chosen strengths, cdef_p<n>_*.avif = same stream with the
# strengths/damping rewritten by patch_cdef.py (ffmpeg stays the reference).
set -eu
O="${1:?outdir}"
H="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$O"
enc() { # name size crf cpu seed
  ffmpeg -v error -y -f lavfi -i "gradients=s=$2:d=1:seed=$5,noise=alls=${6:-14}:allf=t:all_seed=$5" -frames:v 1 -pix_fmt yuv444p -c:v libaom-av1 -crf "$3" -cpu-used "$4" \
    -still-picture 1 -aom-params enable-cdef=1:enable-restoration=0:enable-palette=0:sb-size=64 "$O/$1.avif"
}
enc cdef_a 200x136 22 0 11
enc cdef_b 160x120 30 1 12
enc cdef_c 256x192 24 2 13
enc cdef_d 203x139 35 3 14
enc cdef_e 131x77 28 1 15
enc cdef_f 320x240 20 0 16 18
enc cdef_g 96x72 50 2 17 6
# smooth background + one noisy patch (PPM written by python, fixed seed): many skip blocks, so the "all four 4x4 skip" test of the 8x8 filter is exercised
encskip() { # name size crf cpu seed
  python3 -I - "$O/$1.ppm" "$2" "$5" <<'PY'
import random, sys
out, size, seed = sys.argv[1], sys.argv[2], int(sys.argv[3])
w, h = (int(v) for v in size.split("x"))
rnd = random.Random(seed)
rows = []
for y in range(h):
    row = bytearray()
    for x in range(w):
        if 60 <= x < 60 + 88 and 30 <= y < 30 + 72:
            v = [min(255, max(0, 90 + (x * 3 + y) % 60 + rnd.randrange(-30, 31))) for _ in range(3)]
        else:
            v = [60 + x * 100 // w, 100 + y * 80 // h, 200 - x * 60 // w]
        row += bytes(v)
    rows.append(bytes(row))
open(out, "wb").write(b"P6\n%d %d\n255\n" % (w, h) + b"".join(rows))
PY
  ffmpeg -v error -y -i "$O/$1.ppm" -frames:v 1 -pix_fmt yuv444p -c:v libaom-av1 -crf "$3" -cpu-used "$4" \
    -still-picture 1 -aom-params enable-cdef=1:enable-restoration=0:enable-palette=0:sb-size=64 "$O/$1.avif"
  rm -f "$O/$1.ppm"
}
# vertical stripes (per-column random values): V_PRED predicts every row below the first block row exactly, so those blocks are skip=1
# although the content is full of strong edges that CDEF would change
encstripes() { # name size crf cpu seed
  python3 -I - "$O/$1.ppm" "$2" "$5" <<'PY'
import random, sys
out, size, seed = sys.argv[1], sys.argv[2], int(sys.argv[3])
w, h = (int(v) for v in size.split("x"))
rnd = random.Random(seed)
col = [tuple(rnd.randrange(256) for _ in range(3)) for _ in range(w)]
row = b"".join(bytes(c) for c in col)
open(out, "wb").write(b"P6\n%d %d\n255\n" % (w, h) + row * h)
PY
  ffmpeg -v error -y -i "$O/$1.ppm" -frames:v 1 -pix_fmt yuv444p -c:v libaom-av1 -crf "$3" -cpu-used "$4" \
    -still-picture 1 -aom-params enable-cdef=1:enable-restoration=0:enable-palette=0:sb-size=64 "$O/$1.avif"
  rm -f "$O/$1.ppm"
}
encstripes cdef_v1 192x128 30 1 31
encstripes cdef_v2 136x72 45 2 32
encskip cdef_s1 256x192 40 1 21
encskip cdef_s2 200x150 28 2 22
P="python3 -I $H/patch_cdef.py"
$P "$O/cdef_b.avif" "$O/cdef_p1_damp3.avif" 3 0:5:1:3:1
$P "$O/cdef_b.avif" "$O/cdef_p2_damp6.avif" 6 0:15:3:15:3
$P "$O/cdef_f.avif" "$O/cdef_p3_pri_only.avif" 4 0:11:0:5:0 1:2:0:14:0 2:8:0:1:0 3:15:0:0:0 4:7:0:9:0 5:3:0:12:0 6:13:0:4:0 7:1:0:6:0
$P "$O/cdef_f.avif" "$O/cdef_p4_sec_only.avif" 5 0:0:1:0:2 1:0:2:0:3 2:0:3:0:1 3:0:0:0:0 4:0:1:0:1 5:0:2:0:2 6:0:3:0:3 7:0:1:0:3
$P "$O/cdef_a.avif" "$O/cdef_p5_odd.avif" 4 0:1:1:1:1 1:15:3:7:2
$P "$O/cdef_d.avif" "$O/cdef_p6_mix.avif" 6 0:13:3:2:2 1:6:1:9:3
$P "$O/cdef_e.avif" "$O/cdef_p7_zero_y.avif" 3 0:0:0:15:3
$P "$O/cdef_e.avif" "$O/cdef_p8_zero_uv.avif" 5 0:15:3:0:0
$P "$O/cdef_f.avif" "$O/cdef_p9_big.avif" 4 0:14:1:3:1 1:4:2:12:3 2:10:0:5:0 3:1:3:8:1 4:9:2:2:2 5:5:1:13:0 6:12:3:10:1 7:2:0:15:3
$P "$O/cdef_g.avif" "$O/cdef_p10_damp3_hi.avif" 3 0:15:3:15:3 1:15:3:15:3
$P "$O/cdef_s1.avif" "$O/cdef_p11_skip.avif" 4 0:15:3:15:3 1:12:2:9:2
$P "$O/cdef_s2.avif" "$O/cdef_p12_skip.avif" 5 0:9:1:6:1 1:15:3:15:3
$P "$O/cdef_v1.avif" "$O/cdef_p13_stripes.avif" 4 0:12:2:6:1 1:0:3:15:3
$P "$O/cdef_v2.avif" "$O/cdef_p14_stripes.avif" 3 0:0:2:0:2 1:15:1:15:1
