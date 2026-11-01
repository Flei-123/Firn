#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vp8dec.fi" -o "$D/vp8dec"
bad=0
n=0
run_clips() {
  gopt=$1
  while read -r name src size dur opts; do
    [ -z "$name" ] && continue
    case "$name" in
      t_*)
        for k in 1 2; do
          ffmpeg -nostdin -v error -y -f lavfi -t "$dur" -i "$src=size=$size:rate=25" -pix_fmt yuv420p -c:v libvpx $gopt $opts -pass $k -passlogfile "$D/pass" -f webm "$D/$name.webm" 2>/dev/null
        done ;;
      *)
        ffmpeg -nostdin -v error -y -f lavfi -t "$dur" -i "$src=size=$size:rate=25" -pix_fmt yuv420p -c:v libvpx $gopt $opts "$D/$name.webm" 2>/dev/null ;;
    esac
    ffmpeg -nostdin -v error -y -i "$D/$name.webm" -map 0:v:0 -fps_mode passthrough -f rawvideo -pix_fmt yuv420p "$D/ref.yuv"
    "$D/vp8dec" "$D/$name.webm" "$D/out.yuv" || true
    n=$((n+1))
    if ! cmp -s "$D/out.yuv" "$D/ref.yuv"; then echo "DIFF: $name $size $gopt $opts"; bad=$((bad+1)); fi
  done
}
run_clips "-g 1" <<LIST
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
run_clips "" <<LIST
i_p0 testsrc2 176x144 3 -b:v 400k
i_hq testsrc2 160x96 2 -b:v 2M -cpu-used 0 -deadline best
i_lq testsrc2 160x96 2 -b:v 60k
i_alt testsrc2 176x144 4 -b:v 300k -auto-alt-ref 1 -lag-in-frames 16
i_alt2 mandelbrot 192x112 4 -b:v 400k -auto-alt-ref 1 -lag-in-frames 25 -g 20
i_gold testsrc2 128x96 4 -b:v 200k -g 12 -auto-alt-ref 1 -lag-in-frames 10
i_odd testsrc2 33x17 3 -b:v 100k
i_wide testsrc2 17x300 3 -b:v 200k
i_tiny testsrc2 17x13 3 -b:v 100k
i_parts testsrc2 176x144 3 -b:v 800k -slices 8
i_res testsrc2 160x96 3 -b:v 200k -error-resilient 1
i_seg testsrc2 160x96 3 -deadline realtime -cpu-used 5 -b:v 100k -error-resilient 1
i_rt mandelbrot 192x112 3 -deadline realtime -cpu-used 8 -b:v 80k -static-thresh 1000
i_p1 testsrc2 176x144 3 -b:v 400k -profile:v 1
i_p1_alt testsrc2 176x144 4 -b:v 300k -profile:v 1 -auto-alt-ref 1 -lag-in-frames 16
i_p2 testsrc2 33x17 3 -b:v 200k -profile:v 2
i_p2_big testsrc2 176x144 3 -b:v 400k -profile:v 2
i_p3 testsrc2 321x241 3 -b:v 800k -profile:v 3
i_life life 160x120 3 -b:v 600k
i_rgb rgbtestsrc 200x150 3 -b:v 900k -slices 4
i_cell cellauto 128x96 3 -b:v 700k
i_sharp testsrc2 160x96 3 -b:v 150k -sharpness 7 -qmin 30
i_big testsrc2 1280x720 2 -b:v 3M -slices 4
LIST
run_clips "" <<LIST
t_arf0 mandelbrot 192x112 8 -deadline good -cpu-used 0 -auto-alt-ref 1 -lag-in-frames 25 -g 100 -b:v 300k
t_arf1 testsrc2 176x144 6 -deadline good -cpu-used 1 -auto-alt-ref 1 -lag-in-frames 16 -g 60 -b:v 250k -profile:v 1
t_arf2 life 160x120 6 -deadline good -cpu-used 0 -auto-alt-ref 1 -lag-in-frames 25 -b:v 500k -profile:v 2
t_arf3 testsrc2 64x48 6 -deadline good -cpu-used 0 -auto-alt-ref 1 -lag-in-frames 25 -b:v 120k -profile:v 3 -arnr-strength 5
t_arf4 rgbtestsrc 100x62 6 -deadline good -cpu-used 0 -auto-alt-ref 1 -lag-in-frames 25 -b:v 300k -slices 4 -g 30
LIST
if [ -n "$FUZZ_RUNS" ]; then
  FIRNLIB=$ROOT/lib ${FIRNC:-firnc} --opt-level=release-safe "$ROOT/tools/media_video/vp8dec.fi" -o "$D/vp8dec_safe"
  python3 -I "$ROOT/tools/media_video/fuzz_vp8.py" "$D" "$D/vp8dec_safe" "$FUZZ_RUNS" || bad=$((bad+1))
fi
echo "vp8 clips=$n bad=$bad"
test $bad -eq 0
