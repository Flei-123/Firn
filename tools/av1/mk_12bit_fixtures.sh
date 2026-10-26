#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
enc() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; cpu="$5"; params="$6"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libaom-av1 -crf "$crf" -cpu-used "$cpu" -still-picture 1 -aom-params "$params" "$O/$name.avif"
}
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-14}:allf=t:all_seed=$2"; }
F=yuv420p12le
enc b12_200 "$(G 200x200 5 10)" $F 30 4 "enable-cdef=1:enable-restoration=1"
enc b12_129 "$(G 129x129 31 12)" $F 25 3 "enable-cdef=1:enable-restoration=0"
enc b12_131x77 "$(G 131x77 32 20)" $F 45 2 "enable-cdef=1:enable-restoration=1"
enc b12_300_lr "$(G 300x300 33 10)" $F 35 3 "enable-cdef=1:enable-restoration=1"
enc b12_lowq "$(G 256x256 34 6)" $F 10 3 "enable-cdef=1:enable-restoration=1"
enc b12_hiq "$(G 256x256 35 30)" $F 55 3 "enable-cdef=1:enable-restoration=1"
enc b12_444 "$(G 200x200 36)" yuv444p12le 30 4 "enable-cdef=1:enable-restoration=1"
enc b12_422 "$(G 200x200 37)" yuv422p12le 30 4 "enable-cdef=1:enable-restoration=1"
enc b12_400 "$(G 200x200 38)" gray12le 30 4 "enable-cdef=1:enable-restoration=0"
enc b12_lossless "$(G 200x200 39)" $F 0 4 "lossless=1"
enc b12_tiles "$(G 400x400 40)" $F 30 4 "enable-cdef=1:enable-restoration=0:tile-columns=1:tile-rows=1"
enc b12_dq "$(G 300x300 41)" $F 30 4 "enable-cdef=1:enable-restoration=0:deltaq-mode=3:delta-lf-mode=1"
enc b12_qm "$(G 300x300 42)" $F 30 4 "enable-cdef=1:enable-restoration=0:enable-qm=1"
enc b12_sb128 "$(G 300x300 43 10)" $F 30 4 "enable-cdef=1:enable-restoration=1:sb-size=128"
enc b12_pal "testsrc2=s=300x300,format=yuv420p12le" $F 30 3 "enable-cdef=1:enable-restoration=0:tune-content=screen"
enc b12_ibc "testsrc2=s=300x300:r=25,tile=3x3,format=yuv420p12le" $F 25 3 "enable-cdef=1:enable-restoration=1:tune-content=screen"
