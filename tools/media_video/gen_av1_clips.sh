#!/bin/bash
D=$1
mkdir -p $D
g() {
  name=$1; src=$2; size=$3; dur=$4; shift 4
  ffmpeg -nostdin -v error -y -f lavfi -t $dur -i "$src=size=$size:rate=25" -pix_fmt ${PIX:-yuv420p} "$@" -f obu $D/$name.obu 2>&1 | head -3
}
g aom_basic testsrc2 64x48 1 -c:v libaom-av1 -cpu-used 8
g aom_lag testsrc2 128x96 2 -c:v libaom-av1 -cpu-used 8 -lag-in-frames 10 -b:v 200k
g aom_tiles testsrc2 704x144 1 -c:v libaom-av1 -cpu-used 8 -tile-columns 2 -tile-rows 1
g aom_tiles_nu testsrc2 640x480 1 -c:v libaom-av1 -cpu-used 8 -tile-columns 2 -tile-rows 2 -aom-params tile-width=4,3:tile-height=3,2
g aom_seg testsrc2 192x112 1 -c:v libaom-av1 -cpu-used 8 -aq-mode 1 -b:v 100k
g aom_deltaq testsrc2 192x112 1 -c:v libaom-av1 -cpu-used 8 -aom-params deltaq-mode=1:enable-tpl-model=1 -lag-in-frames 8
g aom_errres testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -error-resilient 1
g aom_lossless testsrc2 96x64 1 -c:v libaom-av1 -cpu-used 8 -lossless 1
g aom_sb128 testsrc2 256x160 1 -c:v libaom-av1 -cpu-used 8 -aom-params sb-size=128
g aom_superres testsrc2 160x96 1 -c:v libaom-av1 -cpu-used 8 -aom-params resize-mode=0:superres-mode=1
g aom_screen mandelbrot 192x128 1 -c:v libaom-av1 -cpu-used 8 -aom-params tune-content=screen:enable-intrabc=1
g aom_qm testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -aom-params enable-qm=1:qm-min=2:qm-max=10
g aom_fgrain testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -aom-params film-grain-test=3
g aom_fgrain_tbl testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -aom-params film-grain-test=10
g aom_odd testsrc2 17x13 1 -c:v libaom-av1 -cpu-used 8
g aom_cif testsrc2 352x288 1 -c:v libaom-av1 -cpu-used 6 -b:v 300k
g aom_lr testsrc2 192x112 1 -c:v libaom-av1 -cpu-used 4 -aom-params enable-restoration=1 -lag-in-frames 6
g aom_frameparallel testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -frame-parallel 1 -lag-in-frames 8
g aom_refsig testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -aom-params enable-order-hint=0
g aom_noorder testsrc2 128x96 1 -c:v libaom-av1 -cpu-used 8 -aom-params enable-order-hint=0:enable-ref-frame-mvs=0
g aom_ref6 testsrc2 128x96 2 -c:v libaom-av1 -cpu-used 8 -lag-in-frames 16 -g 10
PIX=yuv420p10le g aom_10bit testsrc2 96x64 1 -c:v libaom-av1 -cpu-used 8
PIX=yuv444p g aom_444 testsrc2 96x64 1 -c:v libaom-av1 -cpu-used 8
PIX=yuv422p g aom_422 testsrc2 96x64 1 -c:v libaom-av1 -cpu-used 8
PIX=gray g aom_gray testsrc2 96x64 1 -c:v libaom-av1 -cpu-used 8
PIX=yuv420p12le g aom_12bit testsrc2 96x64 1 -c:v libaom-av1 -cpu-used 8 -strict -2
g svt_basic testsrc2 128x96 2 -c:v libsvtav1 -preset 10
g svt_tiles testsrc2 704x144 1 -c:v libsvtav1 -preset 10 -svtav1-params tile-columns=1:tile-rows=1
g svt_gop testsrc2 192x112 2 -c:v libsvtav1 -preset 8 -g 8 -svtav1-params hierarchical-levels=3
g svt_fg testsrc2 128x96 1 -c:v libsvtav1 -preset 10 -svtav1-params film-grain=8
g rav1e_basic testsrc2 128x96 1 -c:v librav1e -speed 10
g rav1e_tiles testsrc2 512x288 1 -c:v librav1e -speed 10 -tiles 4
g rav1e_still testsrc2 64x48 1 -c:v librav1e -speed 10 -g 1 -rav1e-params still_picture=true
