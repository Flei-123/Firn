#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vdec.fi" -o "$D/vdec"
bad=0
n=0
while read -r name size dur opts; do
  ffmpeg -v error -y -f lavfi -i "testsrc2=size=$size:rate=25:duration=$dur" -c:v libx264 $opts -pix_fmt yuv420p "$D/$name.mp4" 2>/dev/null
  ffmpeg -v error -y -i "$D/$name.mp4" -map 0:v:0 -f rawvideo -pix_fmt yuv420p "$D/ref.yuv"
  "$D/vdec" "$D/$name.mp4" "$D/out.yuv" || true
  n=$((n+1))
  if ! cmp -s "$D/out.yuv" "$D/ref.yuv"; then echo "DIFF: $name $size $opts"; bad=$((bad+1)); fi
done <<LIST
base_cavlc 64x48 2 -profile:v baseline -bf 0
main_cabac 100x62 2 -profile:v main -bf 3 -g 12
main_cavlc_b 176x144 2 -profile:v main -x264-params cabac=0 -bf 3
high_8x8 176x144 2 -profile:v high -x264-params 8x8dct=1 -bf 3 -refs 4
high_crop 321x241 2 -profile:v high -bf 2 -g 10
high_weightp 128x96 3 -profile:v high -weightp 2 -bf 3 -refs 5
high_direct_temporal 128x96 2 -profile:v high -x264-params direct=temporal -bf 4 -b-pyramid normal
high_multislice 192x128 2 -profile:v high -x264-params slices=4
high_intra_only 160x120 1 -profile:v high -g 1
high_720p 1280x720 1 -profile:v high -bf 2 -preset veryfast
LIST
echo "h264 clips=$n bad=$bad"
test $bad -eq 0
