#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vsrc.fi" -o "$D/vsrc"
bad=0
for f in "$ROOT"/testdata/media_video/vp9_*.webm "$ROOT"/testdata/media_video/vp9_*.mp4; do
  ffmpeg -nostdin -v error -y -i "$f" -map 0:v:0 -fps_mode passthrough -f rawvideo -pix_fmt rgba "$D/ref.rgba"
  "$D/vsrc" "$f" "$D/out.rgba" || { echo "FAIL: $f"; bad=$((bad+1)); continue; }
  python3 -I -c "
import sys
a=open(sys.argv[1],'rb').read();b=open(sys.argv[2],'rb').read()
sys.exit(0 if len(a)==len(b) and max(abs(x-y) for x,y in zip(a,b))<=3 else 1)" "$D/out.rgba" "$D/ref.rgba" || { echo "DIFF: $f"; bad=$((bad+1)); }
done
echo "bad=$bad"
[ $bad -eq 0 ]
