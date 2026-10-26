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
enc b10_64_nf "$(G 64x64 41)" yuv420p10le 40 4 "enable-cdef=0:enable-restoration=0:$B"
enc b10_16 "$(G 16x16 42)" yuv420p10le 30 4 "enable-cdef=1:enable-restoration=0:$B"
enc b10_129 "$(G 129x129 43)" yuv420p10le 25 3 "enable-cdef=1:enable-restoration=0:$B"
enc b10_200 "$(G 200x200 44 8)" yuv420p10le 35 3 "enable-cdef=1:enable-restoration=0:$B"
enc b10_131x77 "$(G 131x77 45 20)" yuv420p10le 45 2 "enable-cdef=1:enable-restoration=0:$B"
enc b10_smooth "gradients=s=256x256:d=1:seed=46" yuv420p10le 30 2 "enable-cdef=1:enable-restoration=0:$B"
enc b10_200_lr "$(G 200x200 47 10)" yuv420p10le 35 3 "enable-cdef=1:enable-restoration=1:$B"
enc b10_300_lr "mandelbrot=s=300x300" yuv420p10le 30 3 "enable-cdef=1:enable-restoration=1:$B"
enc b10_200_lr_nocdef "$(G 200x200 48 10)" yuv420p10le 35 3 "enable-cdef=0:enable-restoration=1:$B"
enc b10_200_444 "$(G 200x200 49)" yuv444p10le 30 4 "enable-cdef=1:enable-restoration=1:$B"
enc b10_200_400 "$(G 200x200 50)" gray10le 30 4 "enable-cdef=1:enable-restoration=0:$B"
enc b10_200_lossless "$(G 200x200 51)" yuv420p10le 0 4 "lossless=1:$B"
enc b10_scr_300 "testsrc2=s=300x300,format=yuv420p10le,eq=contrast=1.4" yuv420p10le 25 3 "enable-cdef=1:enable-restoration=0:sb-size=64:tune-content=screen"
enc b10_200_tiles "$(G 200x200 52)" yuv420p10le 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=1"
enc b10_200_dq "$(G 200x200 53)" yuv420p10le 30 4 "enable-cdef=1:enable-restoration=0:$B:deltaq-mode=1"
enc b10_200_qm "$(G 200x200 54)" yuv420p10le 30 4 "enable-cdef=1:enable-restoration=0:$B:enable-qm=1"
enc b10_lowq "$(G 128x128 55 6)" yuv420p10le 10 3 "enable-cdef=1:enable-restoration=1:$B"
enc b10_hiq "$(G 128x128 56 30)" yuv420p10le 55 3 "enable-cdef=1:enable-restoration=1:$B"
