#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vsrc.fi" -o "$D/vsrc"
bad=0
for f in "$ROOT"/testdata/media_video/av1_*.webm "$ROOT"/testdata/media_video/av1_*.mp4; do
  case "$f" in
    *10bit*)
      ffmpeg -nostdin -v error -y -i "$f" -map 0:v:0 -fps_mode passthrough -f rawvideo -pix_fmt yuv420p10le "$D/ref.yuv"
      read -r W H < <(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$f" | tr ',' ' ')
      python3 -I "$ROOT/tools/media_video/yuv10_to_rgba.py" "$D/ref.yuv" "$D/ref.rgba" "$W" "$H" ;;
    *) ffmpeg -nostdin -v error -y -i "$f" -map 0:v:0 -fps_mode passthrough -f rawvideo -pix_fmt rgba "$D/ref.rgba" ;;
  esac
  "$D/vsrc" "$f" "$D/out.rgba" || { echo "FAIL: $f"; bad=$((bad+1)); continue; }
  python3 -I -c "
import sys
a=open(sys.argv[1],'rb').read();b=open(sys.argv[2],'rb').read()
if len(a)!=len(b): sys.exit(1)
d=[abs(x-y) for x,y in zip(a,b)]
sys.exit(0 if max(d)<=3 else 1)" "$D/out.rgba" "$D/ref.rgba" || { echo "DIFF: $f"; bad=$((bad+1)); }
done
echo "bad=$bad"
[ $bad -eq 0 ]
