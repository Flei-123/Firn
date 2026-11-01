#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} -o "$D/av1dec" "$ROOT/tools/media_video/av1dec.fi"
BASE="enable-palette=0:enable-intrabc=0:enable-cdef=0:enable-restoration=0:loopfilter-control=0"
PASS=0
FAIL=0
run_case() {
  local name=$1 src=$2 size=$3 pix=$4 frames=$5 params=$6
  shift 6
  local w=${size%x*} h=${size#*x} bps=1 ssx=1 ssy=1
  case $pix in
    *10le) bps=2 ;;
  esac
  case $pix in
    yuv444*) ssx=0; ssy=0 ;;
  esac
  ffmpeg -nostdin -v error -y -f lavfi -i "$src=size=$size:rate=25" -frames:v $frames -pix_fmt $pix -c:v libaom-av1 -g 1 -cpu-used 8 -aom-params "$params" "$@" "$D/$name.ivf" >/dev/null 2>&1 || { echo "SKIP $name (encode)"; return; }
  ffmpeg -nostdin -v error -y -c:v libdav1d -i "$D/$name.ivf" -f rawvideo -pix_fmt $pix "$D/$name.ref" >/dev/null 2>&1
  "$D/av1dec" "$D/$name.ivf" "$D/$name.yuv" "$D/$name.info" || true
  if python3 -I "$ROOT/tools/media_video/av1yuv_cmp.py" "$D/$name.yuv" "$D/$name.ref" $w $h $bps $ssx $ssy; then
    PASS=$((PASS+1))
    echo "ok   $name"
  else
    FAIL=$((FAIL+1))
    echo "FAIL $name $(grep -h 'err=\|err_mi\|err_tile\|postfilter' "$D/$name.info" | paste -sd' ')"
  fi
}
for s in testsrc2:64x48 testsrc2:128x96 mandelbrot:96x64 testsrc2:352x288 testsrc2:17x13 testsrc2:200x130; do
  src=${s%%:*}; size=${s#*:}
  run_case ll_${src}_$size $src $size yuv420p 1 "lossless=1:$BASE"
  run_case q_${src}_$size $src $size yuv420p 1 "$BASE" -crf 20 -b:v 0
  run_case hq_${src}_$size $src $size yuv420p 1 "$BASE" -crf 3 -b:v 0
  run_case lq_${src}_$size $src $size yuv420p 1 "$BASE" -crf 55 -b:v 0
done
run_case multi testsrc2 128x96 yuv420p 4 "$BASE" -crf 25 -b:v 0
run_case sb128 testsrc2 256x160 yuv420p 1 "$BASE:sb-size=128" -crf 25 -b:v 0
run_case tiles testsrc2 704x144 yuv420p 1 "$BASE" -crf 25 -b:v 0 -tile-columns 2 -tile-rows 1
run_case q10 testsrc2 96x64 yuv420p10le 1 "$BASE" -crf 25 -b:v 0
run_case ll10 testsrc2 96x64 yuv420p10le 1 "lossless=1:$BASE"
run_case c444 testsrc2 96x64 yuv444p 1 "$BASE" -crf 25 -b:v 0
run_case qm testsrc2 128x96 yuv420p 1 "$BASE:enable-qm=1:qm-min=2:qm-max=10" -crf 25 -b:v 0
run_case deltaq testsrc2 192x112 yuv420p 1 "$BASE:deltaq-mode=1" -crf 25 -b:v 0
run_case aq testsrc2 192x112 yuv420p 1 "$BASE" -aq-mode 1 -crf 30 -b:v 0
echo "pass=$PASS fail=$FAIL"
[ $FAIL -eq 0 ]
