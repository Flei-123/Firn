#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} -o "$D/av1dec" "$ROOT/tools/media_video/av1dec.fi"
PASS=0
FAIL=0
geometry() {
  case $1 in
    yuv444*) echo "0 0" ;;
    yuv422*) echo "1 0" ;;
    *) echo "1 1" ;;
  esac
}
bytes_per_sample() {
  case $1 in
    *10le|*12le) echo 2 ;;
    *) echo 1 ;;
  esac
}
compare() {
  local name=$1 size=$2 pix=$3 var=$4
  local w=${size%x*} h=${size#*x}
  local bps ssx ssy
  bps=$(bytes_per_sample $pix)
  read ssx ssy <<< "$(geometry $pix)"
  ffmpeg -nostdin -v error -y -c:v ${REFDEC:-libdav1d} -i "$D/$name.ivf" -fps_mode passthrough -autoscale 0 -f rawvideo -pix_fmt $pix "$D/$name.ref" >/dev/null 2>&1
  "$D/av1dec" "$D/$name.ivf" "$D/$name.yuv" "$D/$name.info" || true
  [ "$var" = 1 ] && w=0
  if python3 -I "$ROOT/tools/media_video/av1yuv_cmp.py" "$D/$name.yuv" "$D/$name.ref" $w $h $bps $ssx $ssy; then
    PASS=$((PASS+1))
    echo "ok   $name"
  else
    FAIL=$((FAIL+1))
    echo "FAIL $name $(grep -h 'err=\|err_mi\|err_tile' "$D/$name.info" | paste -sd' ')"
  fi
  rm -f "$D/$name.ref" "$D/$name.yuv" "$D/$name.ivf"
}
want() {
  [ -z "$ONLY" ] || [[ $1 == $ONLY ]]
}
aom_clip() {
  local name=$1 src=$2 size=$3 pix=$4 frames=$5 params=$6
  shift 6
  want $name || return 0
  ffmpeg -nostdin -v error -y -f lavfi -i "$src=size=$size:rate=25" -frames:v $frames -pix_fmt $pix -c:v libaom-av1 -g ${GOP:-30} -cpu-used 8 -aom-params "$params" "$@" "$D/$name.ivf" >/dev/null 2>&1 || { echo "SKIP $name (encode)"; return; }
  compare $name $size $pix 0
}
svt_clip() {
  local name=$1 src=$2 size=$3 pix=$4 frames=$5 params=$6
  shift 6
  want $name || return 0
  ffmpeg -nostdin -v error -y -f lavfi -i "$src=size=$size:rate=25" -frames:v $frames -pix_fmt $pix -c:v libsvtav1 -g ${GOP:-30} -preset 8 -svtav1-params "scm=0:$params" "$@" "$D/$name.ivf" >/dev/null 2>&1 || { echo "SKIP $name (encode)"; return; }
  compare $name $size $pix 1
}
Q="-crf 30 -b:v 0"
aom_clip hd_tiles testsrc2 700x150 yuv420p 8 "" $Q -tile-columns 2 -tile-rows 1
aom_clip hd_tiles_sb128 testsrc2 700x300 yuv420p 6 "sb-size=128" $Q -tile-columns 1 -tile-rows 1
aom_clip hd_odd testsrc2 173x131 yuv420p 8 "" $Q
aom_clip hd_odd_small testsrc2 33x17 yuv420p 8 "" $Q
aom_clip hd_odd_444 testsrc2 75x41 yuv444p 8 "" $Q
aom_clip hd_er testsrc2 176x144 yuv420p 8 "error-resilient=1" $Q
aom_clip hd_aq1 testsrc2 176x144 yuv420p 8 "aq-mode=1" $Q
aom_clip hd_aq2 testsrc2 176x144 yuv420p 8 "aq-mode=2" $Q
aom_clip hd_aq3 testsrc2 176x144 yuv420p 8 "aq-mode=3" $Q
aom_clip hd_deltaq testsrc2 176x144 yuv420p 8 "deltaq-mode=1" $Q
aom_clip hd_12bit testsrc2 96x64 yuv420p12le 6 "" $Q
aom_clip hd_12bit_444 testsrc2 96x64 yuv444p12le 6 "" $Q -strict -2
aom_clip hd_422 testsrc2 96x64 yuv422p 6 "" $Q -strict -2
aom_clip hd_422_10 testsrc2 96x64 yuv422p10le 6 "" $Q -strict -2
aom_clip hd_422_12 testsrc2 96x64 yuv422p12le 6 "" $Q -strict -2
aom_clip hd_mono testsrc2 96x64 gray 6 "" $Q
aom_clip hd_mono10 testsrc2 96x64 gray10le 6 "" $Q
GOP=100 aom_clip hd_long testsrc2 160x96 yuv420p 70 "" $Q
GOP=100 aom_clip hd_long_mandel mandelbrot 128x96 yuv420p 64 "" $Q
aom_clip hd_sb128 testsrc2 256x160 yuv420p 8 "sb-size=128" $Q
aom_clip hd_noorderhint testsrc2 176x144 yuv420p 8 "enable-order-hint=0" $Q
aom_clip hd_nolag testsrc2 176x144 yuv420p 8 "" $Q -lag-in-frames 0
aom_clip hd_sframe testsrc2 176x144 yuv420p 16 "sframe-dist=5:sframe-mode=1" $Q
aom_clip hd_screen testsrc2 176x144 yuv420p 8 "tune-content=screen" $Q
aom_clip hd_rt testsrc2 176x144 yuv420p 8 "" $Q -usage realtime
aom_clip hd_fastdecode testsrc2 176x144 yuv420p 8 "" $Q -cpu-used 1
aom_clip hd_loop_off testsrc2 176x144 yuv420p 8 "enable-cdef=0:enable-restoration=0" $Q
aom_clip hd_gm testsrc2 176x144 yuv420p 8 "enable-global-motion=1:enable-warped-motion=1" $Q
aom_clip hd_fg testsrc2 160x96 yuv420p 6 "film-grain-test=3" $Q
svt_clip hd_rs_fixed testsrc2 176x144 yuv420p 10 "resize-mode=1:resize-denom=12:resize-kf-denom=12" 
svt_clip hd_rs_random testsrc2 176x144 yuv420p 12 "resize-mode=2"
svt_clip hd_rs_random2 mandelbrot 200x130 yuv420p 12 "resize-mode=2"
svt_clip hd_rs_dynamic testsrc2 176x144 yuv420p 12 "resize-mode=3"
svt_clip hd_sres_inter testsrc2 176x144 yuv420p 10 "superres-mode=1:superres-denom=12:superres-kf-denom=12"
svt_clip hd_sres_rand testsrc2 176x144 yuv420p 10 "superres-mode=2"
REFDEC=libaom-av1 svt_clip hd_sres_both testsrc2 176x144 yuv420p 12 "superres-mode=1:superres-denom=10:superres-kf-denom=10:resize-mode=2"
svt_clip hd_sres_10 testsrc2 176x144 yuv420p10le 10 "superres-mode=1:superres-denom=14:superres-kf-denom=14"
echo "pass=$PASS fail=$FAIL"
[ $FAIL -eq 0 ]
