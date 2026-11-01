#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} -o "$D/av1dec" "$ROOT/tools/media_video/av1dec.fi"
NOPF="enable-palette=0:enable-intrabc=0:enable-cdef=0:enable-restoration=0"
BASE="$NOPF:loopfilter-control=0"
PASS=0
FAIL=0
run_case() {
  local name=$1 src=$2 size=$3 pix=$4 frames=$5 params=$6
  shift 6
  [ -z "$ONLY" ] || [[ $name == $ONLY ]] || return 0
  local w=${size%x*} h=${size#*x} bps=1 ssx=1 ssy=1
  case $pix in
    *10le|*12le) bps=2 ;;
  esac
  case $pix in
    yuv444*) ssx=0; ssy=0 ;;
    yuv422*) ssy=0 ;;
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
run_screen() {
  local name=$1 w=$2 h=$3 pix=$4 seed=$5 params=$6
  [ -z "$ONLY" ] || [[ $name == $ONLY ]] || return 0
  shift 6
  local bps=1 ssx=1 ssy=1
  case $pix in
    *10le|*12le) bps=2 ;;
  esac
  case $pix in
    yuv444*) ssx=0; ssy=0 ;;
    yuv422*) ssy=0 ;;
  esac
  python3 -I "$ROOT/tools/media_video/gen_av1_screen.py" "$D/$name.png" $w $h $seed ${SCREEN_REPEAT:-}
  ffmpeg -nostdin -v error -y -i "$D/$name.png" -frames:v 1 -pix_fmt $pix -c:v libaom-av1 -g 1 -cpu-used ${SCREEN_CPU:-4} -aom-params "$params" "$@" "$D/$name.ivf" >/dev/null 2>&1 || { echo "SKIP $name (encode)"; return; }
  ffmpeg -nostdin -v error -y -c:v libdav1d -i "$D/$name.ivf" -f rawvideo -pix_fmt $pix "$D/$name.ref" >/dev/null 2>&1
  "$D/av1dec" "$D/$name.ivf" "$D/$name.yuv" "$D/$name.info" || true
  if python3 -I "$ROOT/tools/media_video/av1yuv_cmp.py" "$D/$name.yuv" "$D/$name.ref" $w $h $bps $ssx $ssy; then
    PASS=$((PASS+1))
    echo "ok   $name $(grep -h 'n_palette\|n_intrabc' "$D/$name.info" | paste -sd' ')"
  else
    FAIL=$((FAIL+1))
    echo "FAIL $name $(grep -h 'err=\|err_mi\|err_tile' "$D/$name.info" | paste -sd' ')"
  fi
}
run_svt() {
  local name=$1 size=$2 pix=$3 denom=$4
  [ -z "$ONLY" ] || [[ $name == $ONLY ]] || return 0
  shift 4
  local w=${size%x*} h=${size#*x} bps=1
  case $pix in
    *10le) bps=2 ;;
  esac
  ffmpeg -nostdin -v error -y -f lavfi -i "testsrc2=size=$size:rate=25" -frames:v 1 -pix_fmt $pix -c:v libsvtav1 -g 1 -preset 8 -svtav1-params "scm=0:superres-mode=1:superres-denom=$denom:superres-kf-denom=$denom" "$@" "$D/$name.ivf" >/dev/null 2>&1 || { echo "SKIP $name (encode)"; return; }
  ffmpeg -nostdin -v error -y -c:v libdav1d -i "$D/$name.ivf" -f rawvideo -pix_fmt $pix "$D/$name.ref" >/dev/null 2>&1
  "$D/av1dec" "$D/$name.ivf" "$D/$name.yuv" "$D/$name.info" || true
  if python3 -I "$ROOT/tools/media_video/av1yuv_cmp.py" "$D/$name.yuv" "$D/$name.ref" $w $h $bps 1 1; then
    PASS=$((PASS+1))
    echo "ok   $name"
  else
    FAIL=$((FAIL+1))
    echo "FAIL $name $(grep -h 'err=\|postfilter' "$D/$name.info" | paste -sd' ')"
  fi
}
run_inter() {
  local name=$1 src=$2 size=$3 pix=$4 frames=$5 params=$6
  shift 6
  [ -z "$ONLY" ] || [[ $name == $ONLY ]] || return 0
  local w=${size%x*} h=${size#*x} bps=1 ssx=1 ssy=1
  case $pix in
    *10le|*12le) bps=2 ;;
  esac
  case $pix in
    yuv444*) ssx=0; ssy=0 ;;
    yuv422*) ssy=0 ;;
  esac
  ffmpeg -nostdin -v error -y -f lavfi -i "$src=size=$size:rate=25" -frames:v $frames -pix_fmt $pix -c:v libaom-av1 -g 30 -cpu-used 8 -aom-params "$params" "$@" "$D/$name.ivf" >/dev/null 2>&1 || { echo "SKIP $name (encode)"; return; }
  ffmpeg -nostdin -v error -y -c:v libdav1d -i "$D/$name.ivf" -f rawvideo -pix_fmt $pix "$D/$name.ref" >/dev/null 2>&1
  "$D/av1dec" "$D/$name.ivf" "$D/$name.yuv" "$D/$name.info" || true
  if python3 -I "$ROOT/tools/media_video/av1yuv_cmp.py" "$D/$name.yuv" "$D/$name.ref" $w $h $bps $ssx $ssy; then
    PASS=$((PASS+1))
    echo "ok   $name"
  else
    FAIL=$((FAIL+1))
    echo "FAIL $name $(grep -h 'err=\|err_mi\|err_tile' "$D/$name.info" | paste -sd' ')"
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
for s in testsrc2:64x48 testsrc2:128x96 mandelbrot:96x64 testsrc2:352x288 testsrc2:17x13 testsrc2:200x130; do
  src=${s%%:*}; size=${s#*:}
  run_case lf_${src}_$size $src $size yuv420p 1 "$NOPF" -crf 25 -b:v 0
  run_case lfhq_${src}_$size $src $size yuv420p 1 "$NOPF" -crf 8 -b:v 0
  run_case lflq_${src}_$size $src $size yuv420p 1 "$NOPF" -crf 60 -b:v 0
done
run_case lf_sharp testsrc2 192x112 yuv420p 1 "$NOPF:sharpness=5" -crf 40 -b:v 0
run_case lf_sb128 testsrc2 256x160 yuv420p 1 "$NOPF:sb-size=128" -crf 40 -b:v 0
run_case lf_tiles testsrc2 704x144 yuv420p 1 "$NOPF" -crf 40 -b:v 0 -tile-columns 2 -tile-rows 1
run_case lf_10 testsrc2 96x64 yuv420p10le 1 "$NOPF" -crf 40 -b:v 0
run_case lf_444 testsrc2 96x64 yuv444p 1 "$NOPF" -crf 40 -b:v 0
run_case lf_deltaq testsrc2 192x112 yuv420p 1 "$NOPF:deltaq-mode=1:delta-lf-mode=1" -crf 30 -b:v 0
run_case lf_aq testsrc2 192x112 yuv420p 1 "$NOPF" -aq-mode 1 -crf 40 -b:v 0
run_case lf_multi testsrc2 128x96 yuv420p 4 "$NOPF" -crf 40 -b:v 0
FLT="enable-palette=0:enable-intrabc=0"
for s in testsrc2:128x96 mandelbrot:200x130 testsrc2:352x288 testsrc2:17x13 testsrc2:704x144; do
  src=${s%%:*}; size=${s#*:}
  run_case cdef_${src}_$size $src $size yuv420p 1 "$FLT:enable-restoration=0" -crf 30 -b:v 0
  run_case cdefhq_${src}_$size $src $size yuv420p 1 "$FLT:enable-restoration=0" -crf 10 -b:v 0
  run_case lr_${src}_$size $src $size yuv420p 1 "$FLT:enable-cdef=0" -cpu-used 2 -crf 40 -b:v 0
  run_case flt_${src}_$size $src $size yuv420p 1 "$FLT" -cpu-used 2 -crf 35 -b:v 0
done
for sg in 1 2 4; do
  run_case lrw_blur$sg testsrc2 320x240 yuv420p 1 "$FLT" -cpu-used 1 -crf 50 -b:v 0 -vf "gblur=sigma=$sg,noise=alls=8"
done
run_case lrw_blur10 testsrc2 160x128 yuv420p10le 1 "$FLT" -cpu-used 1 -crf 50 -b:v 0 -vf "gblur=sigma=2,noise=alls=8"
run_case flt_10 testsrc2 96x64 yuv420p10le 1 "$FLT" -cpu-used 2 -crf 30 -b:v 0
run_case flt_12 testsrc2 96x64 yuv420p12le 1 "$FLT" -cpu-used 2 -crf 40 -b:v 0 -strict -2
run_case flt_444 testsrc2 96x64 yuv444p 1 "$FLT" -cpu-used 2 -crf 30 -b:v 0
run_case flt_422 testsrc2 96x64 yuv422p 1 "$FLT" -cpu-used 2 -crf 40 -b:v 0 -strict -2
run_case flt_gray testsrc2 96x64 gray 1 "$FLT" -cpu-used 2 -crf 40 -b:v 0
run_case flt_sb128 testsrc2 256x160 yuv420p 1 "$FLT:sb-size=128" -cpu-used 2 -crf 40 -b:v 0
run_case flt_tiles testsrc2 704x144 yuv420p 1 "$FLT" -cpu-used 2 -crf 40 -b:v 0 -tile-columns 2 -tile-rows 1
run_case flt_multi testsrc2 128x96 yuv420p 4 "$FLT" -cpu-used 2 -crf 40 -b:v 0
run_case flt_noise testsrc2 320x240 yuv420p 1 "$FLT" -cpu-used 2 -crf 45 -b:v 0 -vf noise=alls=20
run_svt sr_160x96 160x96 yuv420p 12
run_svt sr_200x130 200x130 yuv420p 16
run_svt sr_352x288 352x288 yuv420p 9 -crf 40
run_svt sr_10bit 96x64 yuv420p10le 11
run_svt sr_321x177 321x177 yuv420p 10 -crf 50
run_svt sr_704x144 704x144 yuv420p 14
run_svt sr_256x160 256x160 yuv420p 12 -crf 20
run_svt sr_640x360 640x360 yuv420p 11 -crf 45
SC="tune-content=screen:enable-palette=1:enable-intrabc=0"
NOF="enable-cdef=0:enable-restoration=0:loopfilter-control=0"
run_screen pal_192x128 192 128 yuv420p 11 "$SC:$NOF" -crf 20 -b:v 0
run_screen pal_200x130 200 130 yuv420p 12 "$SC:$NOF" -crf 35 -b:v 0
run_screen pal_352x288 352 288 yuv420p 13 "$SC:$NOF" -crf 10 -b:v 0
run_screen pal_10 96 64 yuv420p10le 14 "$SC:$NOF" -crf 25 -b:v 0
run_screen pal_12 96 64 yuv420p12le 15 "$SC:$NOF" -crf 30 -b:v 0 -strict -2
run_screen pal_444 96 64 yuv444p 16 "$SC:$NOF" -crf 25 -b:v 0
run_screen pal_422 96 64 yuv422p 17 "$SC:$NOF" -crf 25 -b:v 0 -strict -2
BC="tune-content=screen:enable-palette=1:enable-intrabc=1"
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_192x128 192 128 yuv420p 21 "$BC:$NOF" -crf 20 -b:v 0
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_200x130 200 130 yuv420p 22 "$BC:$NOF" -crf 35 -b:v 0
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_352x288 352 288 yuv420p 23 "$BC:$NOF" -crf 10 -b:v 0
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_10 224 160 yuv420p10le 24 "$BC:$NOF" -crf 25 -b:v 0
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_12 224 160 yuv420p12le 25 "$BC:$NOF" -crf 30 -b:v 0 -strict -2
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_444 224 160 yuv444p 26 "$BC:$NOF" -crf 25 -b:v 0
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_422 224 160 yuv422p 27 "$BC:$NOF" -crf 25 -b:v 0 -strict -2
SCREEN_CPU=1 SCREEN_REPEAT=repeat run_screen ibc_lossless 224 160 yuv420p 28 "$BC:lossless=1" -b:v 0
run_screen pal_gray 96 64 gray 18 "$SC:$NOF" -crf 25 -b:v 0
run_screen pal_sb128 256 160 yuv420p 19 "$SC:$NOF:sb-size=128" -crf 25 -b:v 0
run_screen pal_17x13 17 13 yuv420p 20 "$SC:$NOF" -crf 25 -b:v 0
run_screen pal_flt 320 240 yuv420p 21 "$SC" -crf 30 -b:v 0 -cpu-used 2
run_screen pal_lossless 192 128 yuv420p 22 "$SC:lossless=1:$NOF" -b:v 0
run_screen pal_tiles 704 144 yuv420p 23 "$SC:$NOF" -crf 30 -b:v 0 -tile-columns 2 -tile-rows 1
FGP="enable-palette=0:enable-intrabc=0"
for k in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; do
  run_case fg_t$k testsrc2 200x130 yuv420p 1 "$FGP:film-grain-test=$k" -cpu-used 4 -crf 30 -b:v 0
done
run_case fg_t1_352 testsrc2 352x288 yuv420p 1 "$FGP:film-grain-test=1" -crf 30 -b:v 0
run_case fg_t5_352 mandelbrot 352x288 yuv420p 1 "$FGP:film-grain-test=5" -crf 30 -b:v 0
run_case fg_odd testsrc2 17x13 yuv420p 1 "$FGP:film-grain-test=2" -crf 30 -b:v 0
run_case fg_odd2 testsrc2 131x75 yuv420p 1 "$FGP:film-grain-test=7" -crf 30 -b:v 0
run_case fg_tiles testsrc2 704x144 yuv420p 1 "$FGP:film-grain-test=3" -crf 30 -b:v 0 -tile-columns 2
run_case fg_10 testsrc2 96x64 yuv420p10le 1 "$FGP:film-grain-test=4" -crf 30 -b:v 0
run_case fg_10b testsrc2 200x130 yuv420p10le 1 "$FGP:film-grain-test=9" -crf 30 -b:v 0
run_case fg_12 testsrc2 96x64 yuv420p12le 1 "$FGP:film-grain-test=6" -crf 30 -b:v 0 -strict -2
run_case fg_444 testsrc2 96x64 yuv444p 1 "$FGP:film-grain-test=8" -crf 30 -b:v 0
run_case fg_444b testsrc2 200x130 yuv444p 1 "$FGP:film-grain-test=1" -crf 30 -b:v 0
run_case fg_422 testsrc2 96x64 yuv422p 1 "$FGP:film-grain-test=10" -crf 30 -b:v 0 -strict -2
run_case fg_gray testsrc2 96x64 gray 1 "$FGP:film-grain-test=11" -crf 30 -b:v 0
run_case fg_multi testsrc2 128x96 yuv420p 6 "$FGP:film-grain-test=12" -crf 30 -b:v 0
run_case fg_sb128 testsrc2 256x160 yuv420p 1 "$FGP:film-grain-test=13:sb-size=128" -crf 30 -b:v 0
for lvl in 10 25 40; do
  run_case fg_dn$lvl testsrc2 320x240 yuv420p 2 "$FGP:denoise-noise-level=$lvl" -crf 30 -b:v 0 -vf noise=alls=$lvl
done
run_case fg_dn_444 testsrc2 160x96 yuv444p 2 "$FGP:denoise-noise-level=30" -crf 30 -b:v 0 -vf noise=alls=30
run_case fg_dn_10 testsrc2 160x96 yuv420p10le 2 "$FGP:denoise-noise-level=30" -crf 30 -b:v 0 -vf noise=alls=30
IOFF="$BASE:enable-obmc=0:enable-warped-motion=0:enable-global-motion=0:enable-masked-comp=0:enable-interintra-comp=0:enable-dist-wtd-comp=0:enable-dual-filter=0:enable-ref-frame-mvs=0"
run_inter in_simple testsrc2 176x144 yuv420p 8 "$IOFF" -crf 30 -b:v 0
run_inter in_mfmv testsrc2 176x144 yuv420p 8 "$IOFF:enable-ref-frame-mvs=1" -crf 30 -b:v 0
run_inter in_masked testsrc2 176x144 yuv420p 8 "$BASE:enable-obmc=0:enable-warped-motion=0:enable-global-motion=0" -crf 30 -b:v 0
run_inter in_obmc testsrc2 176x144 yuv420p 8 "$BASE:enable-warped-motion=0:enable-global-motion=0" -crf 30 -b:v 0
run_inter in_notools testsrc2 176x144 yuv420p 8 "$BASE" -crf 30 -b:v 0
run_inter in_default testsrc2 176x144 yuv420p 8 "" -crf 30 -b:v 0
run_inter in_mandel mandelbrot 200x130 yuv420p 10 "" -crf 25 -b:v 0
run_inter in_444 testsrc2 96x64 yuv444p 6 "" -crf 30 -b:v 0
run_inter in_10 testsrc2 96x64 yuv420p10le 6 "" -crf 30 -b:v 0
echo "pass=$PASS fail=$FAIL"
[ $FAIL -eq 0 ]
