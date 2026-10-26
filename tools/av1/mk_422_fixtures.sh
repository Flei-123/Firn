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
S="enable-cdef=1:enable-restoration=1:sb-size=64:tune-content=screen"
G() { echo "gradients=s=$1:d=1:seed=$2,noise=alls=${3:-14}:allf=t:all_seed=$2"; }
MOSAIC() { echo "testsrc2=s=$1:r=25,tile=$2,format=$3"; }
enc q422_16 "$(G 16x16 71)" yuv422p 30 4 "enable-cdef=1:enable-restoration=0:$B"
enc q422_129 "$(G 129x129 72)" yuv422p 25 3 "enable-cdef=1:enable-restoration=0:$B"
enc q422_200 "$(G 200x200 73 8)" yuv422p 35 3 "enable-cdef=1:enable-restoration=1:$B"
enc q422_131x77 "$(G 131x77 74 20)" yuv422p 45 2 "enable-cdef=1:enable-restoration=1:$B"
enc q422_lowq "$(G 128x128 75 6)" yuv422p 10 3 "enable-cdef=1:enable-restoration=1:$B"
enc q422_hiq "$(G 128x128 76 30)" yuv422p 55 3 "enable-cdef=1:enable-restoration=1:$B"
enc q422_pal "testsrc2=s=200x200,format=yuv422p" yuv422p 30 3 "enable-cdef=1:enable-restoration=0:sb-size=64:tune-content=screen"
enc q422_10b "$(G 200x200 77)" yuv422p10le 30 4 "enable-cdef=1:enable-restoration=1:$B"
enc q422_lossless "$(G 100x100 78)" yuv422p 0 4 "lossless=1:$B"
enc q422_lossless_10b "$(G 100x100 79)" yuv422p10le 0 4 "lossless=1:$B"
enc q422_tiles "$(G 200x200 80)" yuv422p 30 4 "enable-cdef=1:enable-restoration=0:$B:tile-columns=1:tile-rows=1"
enc q422_dq "$(G 200x200 81)" yuv422p 30 4 "enable-cdef=1:enable-restoration=0:$B:deltaq-mode=1"
enc q422_qm "$(G 200x200 82)" yuv422p 30 4 "enable-cdef=1:enable-restoration=0:$B:enable-qm=1"
enc q422_ibc "$(MOSAIC 64x64 3x3 yuv422p)" yuv422p 25 3 "$S"
enc q422_ibc_10b "$(MOSAIC 64x64 3x3 yuv422p10le)" yuv422p10le 25 3 "$S"
enc q422_ibc_lossless "$(MOSAIC 64x64 3x3 yuv422p)" yuv422p 0 3 "lossless=1:sb-size=64:tune-content=screen"
enc q422_ibc_131x77 "$(MOSAIC 44x26 3x3 yuv422p)" yuv422p 20 3 "$S"
