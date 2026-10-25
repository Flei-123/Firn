#!/bin/bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/av1/mk_lr_fixtures.sh <outdir> -- loop restoration fixtures: libaom 4:4:4 stills with restoration on (CDEF on/off), palette off, 64x64
# superblocks, source = lavfi gradients + noise.  libaom picks wiener / self-guided / none per unit (unit size 128 or 256 by frame size).
set -eu
O="${1:?outdir}"
mkdir -p "$O"
enc() { # name size crf cpu seed cdef noise
  ffmpeg -v error -y -f lavfi -i "gradients=s=$2:d=1:seed=$5,noise=alls=$7:allf=t:all_seed=$5" -frames:v 1 -pix_fmt yuv444p -c:v libaom-av1 -crf "$3" -cpu-used "$4" \
    -still-picture 1 -aom-params enable-cdef=$6:enable-restoration=1:enable-palette=0:sb-size=64 "$O/$1.avif"
}
enc lr_a 200x136 22 0 11 1 14
enc lr_b 320x240 30 1 12 1 10
enc lr_c 400x300 24 0 13 0 18
enc lr_d 131x77 28 1 15 1 14
enc lr_e 257x190 35 2 14 0 14
enc lr_f 512x384 20 0 16 1 18
enc lr_g 96x72 45 2 17 1 6
enc lr_h 640x360 38 1 18 0 12
