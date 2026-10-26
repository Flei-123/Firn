#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
enc() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; cpu="$5"; params="$6"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libaom-av1 -crf "$crf" -cpu-used "$cpu" -still-picture 1 -aom-params "$params" "$O/$name.avif"
}
B="enable-palette=0:sb-size=64"
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-14}:allf=t:all_seed=$2"; }
enc tl_200_c1 "$(G 200x200 21)" yuv420p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1"
enc tl_200_c2 "$(G 200x200 22)" yuv420p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=2"
enc tl_200_r1 "$(G 200x200 23)" yuv420p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-rows=1"
enc tl_200_c1r1 "$(G 200x200 24)" yuv420p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=1"
enc tl_300_c2r2 "$(G 300x300 25)" yuv420p 35 3 "enable-cdef=1:enable-restoration=0:$B:tile-columns=2:tile-rows=2"
enc tl_300_lr "mandelbrot=s=300x300" yuv420p 30 3 "enable-cdef=1:enable-restoration=1:$B:tile-columns=1:tile-rows=1"
enc tl_250x190_c1r2 "$(G 250x190 26 20)" yuv420p 40 2 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=2"
enc tl_200_444 "$(G 200x200 27)" yuv444p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=1"
enc tl_200_400 "$(G 200x200 28)" gray 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=1"
enc tl_200_lossless "$(G 200x200 29)" yuv420p 0 4 "lossless=1:$B:tile-columns=1:tile-rows=1"
enc tl_200_dq "$(G 200x200 30)" yuv420p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=1:deltaq-mode=1"
enc tl_scr_300 "testsrc2=s=300x300,format=yuv420p,eq=contrast=1.4" yuv420p 25 3 "enable-cdef=1:enable-restoration=0:sb-size=64:tile-columns=1:tile-rows=1:tune-content=screen"
enc tl_200_w112 "$(G 200x200 31)" yuv420p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-width=1,2,1:tile-height=3,1"
