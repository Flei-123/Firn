#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vp9dec.fi" -o "$D/vp9dec"
bad=0
n=0
run_clips() {
  gopt=$1
  while read -r name src size dur opts; do
    [ -z "$name" ] && continue
    [ -n "$ONLY" ] && [ "$ONLY" != "$name" ] && continue
    ffmpeg -nostdin -v error -y -f lavfi -t "$dur" -i "$src=size=$size:rate=25" -pix_fmt yuv420p -c:v libvpx-vp9 $gopt $opts "$D/$name.webm" 2>/dev/null
    ffmpeg -nostdin -v error -y -i "$D/$name.webm" -map 0:v:0 -fps_mode passthrough -f rawvideo -pix_fmt yuv420p "$D/ref.yuv"
    rm -f "$D/out.yuv"
    "$D/vp9dec" "$D/$name.webm" "$D/out.yuv" || true
    n=$((n+1))
    if ! cmp -s "$D/out.yuv" "$D/ref.yuv"; then echo "DIFF: $name $size $gopt $opts"; bad=$((bad+1)); fi
  done
}
run_clips "-g 1" <<LIST
i_small testsrc2 64x48 1 -b:v 500k
i_odd testsrc2 100x62 2 -b:v 300k
i_tiny testsrc2 17x13 2 -b:v 200k
i_wide testsrc2 17x300 1 -b:v 300k
i_cif testsrc2 352x288 2 -b:v 1M
i_lowq testsrc2 160x96 1 -crf 55 -b:v 0
i_hiq testsrc2 160x96 1 -crf 4 -b:v 0
i_mandel mandelbrot 192x112 2 -b:v 400k
i_tile2 testsrc2 704x144 2 -b:v 800k -tile-columns 1
i_tile4 testsrc2 1024x96 2 -b:v 800k -tile-columns 2
i_tilerows testsrc2 256x300 2 -b:v 800k -tile-rows 1
i_lossless testsrc2 96x64 1 -lossless 1
i_seg mandelbrot 192x112 2 -b:v 100k -aq-mode 1
i_aq3 testsrc2 192x112 2 -b:v 200k -aq-mode 3
i_sharp testsrc2 160x96 1 -b:v 150k -sharpness 7
i_noerr testsrc2 160x96 1 -b:v 300k -frame-parallel 1
i_err testsrc2 160x96 1 -b:v 300k -error-resilient 1
LIST
run_clips "-g 30 -auto-alt-ref 0 -lag-in-frames 0" <<LIST
p_small testsrc2 64x48 1 -b:v 300k
p_odd testsrc2 100x62 2 -b:v 300k
p_cif testsrc2 352x288 2 -b:v 800k
p_mandel mandelbrot 192x112 2 -b:v 400k
p_tile2 testsrc2 704x144 2 -b:v 800k -tile-columns 1
p_tilerows testsrc2 256x300 2 -b:v 800k -tile-rows 1
p_lossless testsrc2 96x64 1 -lossless 1
p_seg mandelbrot 192x112 2 -b:v 100k -aq-mode 1
p_aq3 testsrc2 192x112 2 -b:v 200k -aq-mode 3
p_noerr testsrc2 160x96 1 -b:v 300k -frame-parallel 1
p_err testsrc2 160x96 1 -b:v 300k -error-resilient 1
p_hiq testsrc2 160x96 1 -crf 4 -b:v 0
p_lowq testsrc2 160x96 1 -crf 55 -b:v 0
p_tiny testsrc2 17x13 2 -b:v 200k
p_wide testsrc2 17x300 1 -b:v 300k
p_rt mandelbrot 192x112 2 -b:v 300k -deadline realtime -cpu-used 6
p_screen testsrc2 256x144 2 -b:v 300k -tune-content screen
p_tile4 testsrc2 1024x96 2 -b:v 800k -tile-columns 2
p_long testsrc2 352x288 4 -b:v 600k
LIST
run_clips "-g 60 -auto-alt-ref 1 -lag-in-frames 10" <<LIST
a_small testsrc2 128x96 2 -b:v 300k
a_cif testsrc2 352x288 2 -b:v 800k
a_mandel mandelbrot 192x112 3 -b:v 400k
a_noerr testsrc2 160x96 2 -b:v 300k -frame-parallel 1
a_odd testsrc2 100x62 3 -b:v 200k
a_tiles testsrc2 704x144 2 -b:v 800k -tile-columns 1
a_arnr mandelbrot 192x112 3 -b:v 300k -arnr-strength 6 -arnr-maxframes 7
a_long testsrc2 352x288 4 -b:v 600k -lag-in-frames 25
LIST
echo "vp9: $n clips, $bad diff"
[ "$bad" -eq 0 ]
