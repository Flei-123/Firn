#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vp8dec.fi" -o "$D/vp8dec"
bad=0
n=0
while read -r name src size dur opts; do
  ffmpeg -nostdin -v error -y -f lavfi -t "$dur" -i "$src=size=$size:rate=25" -pix_fmt yuv420p -c:v libvpx -g 1 $opts "$D/$name.webm" 2>/dev/null
  ffmpeg -nostdin -v error -y -i "$D/$name.webm" -map 0:v:0 -f rawvideo -pix_fmt yuv420p "$D/ref.yuv"
  "$D/vp8dec" "$D/$name.webm" "$D/out.yuv" || true
  n=$((n+1))
  if ! cmp -s "$D/out.yuv" "$D/ref.yuv"; then echo "DIFF: $name $size $opts"; bad=$((bad+1)); fi
done <<LIST
p0_small testsrc2 64x48 1 -b:v 500k
p0_odd testsrc2 100x62 2 -b:v 300k -slices 4
p0_tiny testsrc2 17x13 2 -b:v 200k
p0_wide testsrc2 17x300 1 -b:v 300k -slices 2
p0_parts8 testsrc2 176x144 2 -b:v 1M -slices 8
p0_lowq testsrc2 160x96 1 -crf 50 -b:v 0 -qmin 50 -qmax 63
p0_hiq testsrc2 160x96 1 -crf 4 -b:v 0 -qmax 8
p0_sharp testsrc2 160x96 1 -b:v 150k -sharpness 7 -qmin 30
p0_seg testsrc2 160x96 2 -deadline realtime -cpu-used 5 -b:v 100k -error-resilient 1
p0_rt mandelbrot 192x112 2 -deadline realtime -cpu-used 8 -b:v 80k -static-thresh 1000
p1_simple testsrc2 176x144 2 -b:v 400k -profile:v 1
p1_simple_lf testsrc2 176x144 1 -b:v 100k -profile:v 1 -qmin 40
p2 testsrc2 33x17 2 -b:v 200k -profile:v 2
p3_sharp testsrc2 321x241 1 -b:v 800k -profile:v 3 -sharpness 5
noise_life life 160x120 2 -b:v 600k
noise_rgb rgbtestsrc 200x150 2 -b:v 900k -slices 4
noise_cell cellauto 128x96 2 -b:v 700k
big testsrc2 1280x720 1 -b:v 3M -slices 4
LIST
if [ -n "$FUZZ_RUNS" ]; then
  FIRNLIB=$ROOT/lib ${FIRNC:-firnc} --opt-level=release-safe "$ROOT/tools/media_video/vp8dec.fi" -o "$D/vp8dec_safe"
  python3 -I "$ROOT/tools/media_video/fuzz_vp8.py" "$D" "$D/vp8dec_safe" "$FUZZ_RUNS" || bad=$((bad+1))
fi
echo "vp8 key-frame clips=$n bad=$bad"
test $bad -eq 0
