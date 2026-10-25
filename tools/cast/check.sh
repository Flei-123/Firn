#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/cast/stream_dump.fi" -o "$D/sd"
"$D/sd" "$D/a.h264" "$D/a.yuv" "$D/a.ts"
ffmpeg -v error -i "$D/a.h264" -f rawvideo -pix_fmt yuv420p "$D/h264.yuv"
ffmpeg -v error -i "$D/a.ts" -f rawvideo -pix_fmt yuv420p "$D/ts.yuv"
cmp "$D/a.yuv" "$D/h264.yuv"
cmp "$D/a.yuv" "$D/ts.yuv"
echo "cast ffmpeg oracle: h264 and mpegts decode bit-identical"
