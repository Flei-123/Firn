#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/vprobe.fi" -o "$D/vprobe"
bad=0
n=0
for f in "$@"; do
  "$D/vprobe" "$f" dump | awk '$1=="s"{print $2, $5, $6, $4}' | sort -k1,1n -k4,4n -k2,2n > "$D/ours.txt"
  ffprobe -v error -show_entries packet=stream_index,pts_time,size,flags -of csv=p=0 "$f" | awk -F, 'NF>=4{printf "%d %d %s %d\n", $1, $3, ($4 ~ /K/ ? "K" : "-"), $2*1000000+($2<0?-0.5:0.5)}' | sort -k1,1n -k4,4n -k2,2n > "$D/ref.txt"
  n=$((n+1))
  if ! diff -q <(awk '{print $1,$2,$3}' "$D/ours.txt" | sort) <(awk '{print $1,$2,$3}' "$D/ref.txt" | sort) > /dev/null; then echo "PACKETS DIFFER: $f"; bad=$((bad+1)); fi
  if ! paste -d' ' <(awk '{print $1,$4}' "$D/ours.txt") <(awk '{print $4}' "$D/ref.txt") | awk '{d=$2-$3; if (d<0) d=-d; if (d>600) bad=1} END{exit bad}'; then echo "PTS DIFFER: $f"; bad=$((bad+1)); fi
done
echo "demux files=$n bad=$bad"
test $bad -eq 0
