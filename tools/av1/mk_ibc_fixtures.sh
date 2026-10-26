#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/../.."
O="${1:-tests/data/av1}"
mkdir -p "$O"
enc() {
  name="$1"; src="$2"; fmt="$3"; crf="$4"; cpu="$5"; params="$6"
  ffmpeg -v error -y -f lavfi -i "$src" -frames:v 1 -pix_fmt "$fmt" -c:v libaom-av1 -crf "$crf" -cpu-used "$cpu" -still-picture 1 -aom-params "$params" "$O/$name.avif"
}
S="enable-cdef=1:enable-restoration=1:sb-size=64:tune-content=screen"
MOSAIC() { echo "testsrc2=s=$1:r=25,tile=$2,format=$3"; }
enc ibc_mos_192 "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 25 3 "$S"
enc ibc_mos_192_q40 "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 40 2 "$S"
enc ibc_mos_192_q10 "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 10 3 "$S"
enc ibc_mos_131x77 "$(MOSAIC 44x26 3x3 yuv420p)" yuv420p 20 3 "$S"
enc ibc_mos_444 "$(MOSAIC 64x64 3x3 yuv444p)" yuv444p 25 3 "$S"
enc ibc_mos_400 "$(MOSAIC 64x64 3x3 gray)" gray 25 3 "$S"
enc ibc_mos_10b "$(MOSAIC 64x64 3x3 yuv420p10le)" yuv420p10le 25 3 "$S"
enc ibc_mos_tiles "$(MOSAIC 100x100 3x3 yuv420p)" yuv420p 25 3 "$S:tile-columns=1:tile-rows=1"
enc ibc_mos_dq "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 25 3 "$S:deltaq-mode=1"
enc ibc_mos_qm "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 25 3 "$S:enable-qm=1"
enc ibc_mos_rtx "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 25 3 "$S:reduced-tx-type-set=1"
enc ibc_mos_nopal "$(MOSAIC 64x64 3x3 yuv420p)" yuv420p 25 3 "$S:enable-palette=0"
enc ibc_grid_256 "testsrc2=s=128x128:r=25,tile=2x2,drawgrid=w=16:h=16:t=1:c=white@0.5,format=yuv420p" yuv420p 30 2 "$S"
enc ibc_cells_300 "cellauto=s=300x300:rule=110:full=1:scroll=0,format=yuv420p" yuv420p 25 3 "$S"
enc ibc_rgbtest_200 "rgbtestsrc=s=200x200,tile=1x1,format=yuv420p" yuv420p 25 3 "$S"
