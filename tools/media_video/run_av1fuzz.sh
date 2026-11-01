#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} --opt-level=release-safe -o "$D/av1dec" "$ROOT/tools/media_video/av1dec.fi"
mkdir -p "$D/clips"
mk() {
  local name=$1 src=$2 size=$3 pix=$4 params=$5
  shift 5
  ffmpeg -nostdin -v error -y -f lavfi -i "$src=size=$size:rate=25" -frames:v 6 -pix_fmt $pix -c:v libaom-av1 -g 30 -cpu-used 8 -aom-params "$params" "$@" "$D/clips/$name.ivf" >/dev/null 2>&1 || echo "SKIP $name"
}
Q="-crf 30 -b:v 0"
mk f_default testsrc2 176x144 yuv420p "" $Q
mk f_mandel mandelbrot 128x96 yuv420p "" $Q
mk f_444 testsrc2 96x64 yuv444p "" $Q
mk f_10 testsrc2 96x64 yuv420p10le "" $Q
mk f_12 testsrc2 96x64 yuv420p12le "" $Q
mk f_422 testsrc2 96x64 yuv422p "" $Q -strict -2
mk f_tiles testsrc2 700x150 yuv420p "" $Q -tile-columns 2 -tile-rows 1
mk f_aq testsrc2 176x144 yuv420p "aq-mode=1" $Q
mk f_er testsrc2 176x144 yuv420p "error-resilient=1" $Q
mk f_odd testsrc2 75x41 yuv420p "" $Q
mk f_sb128 testsrc2 256x160 yuv420p "sb-size=128" $Q
ffmpeg -nostdin -v error -y -f lavfi -i testsrc2=size=176x144:rate=25 -frames:v 8 -pix_fmt yuv420p -c:v libsvtav1 -g 30 -preset 8 -svtav1-params "scm=0:resize-mode=2" "$D/clips/f_resize.ivf" >/dev/null 2>&1 || echo "SKIP f_resize"
ffmpeg -nostdin -v error -y -f lavfi -i testsrc2=size=176x144:rate=25 -frames:v 8 -pix_fmt yuv420p -c:v libsvtav1 -g 30 -preset 8 -svtav1-params "scm=0:superres-mode=1:superres-denom=12:superres-kf-denom=12" "$D/clips/f_superres.ivf" >/dev/null 2>&1 || echo "SKIP f_superres"
FUZZ_OUT="${FUZZ_OUT:-$D}" python3 -I "$ROOT/tools/media_video/fuzz_av1.py" "$D/clips" "$D/av1dec" ${FUZZ_RUNS:-2000}
