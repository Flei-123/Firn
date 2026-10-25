#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/../.."
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB="$PWD/lib" "${FIRNC:-/root/firn/compiler/target/release/firnc}" --opt-level=release-safe -o "$D/yuvdump" tools/av1/yuvdump.fi >/dev/null 2>&1 || exit 1
for F in tests/data/av1/*.avif; do
  b=$(basename "$F" .avif)
  ffmpeg -v error -y -i "$F" -f rawvideo "$D/ref.yuv" 2>/dev/null || continue
  "$D/yuvdump" <"$F" >"$D/out.yuv"
  rc=$?
  if [ $rc -eq 0 ]; then
    cmp -s "$D/out.yuv" "$D/ref.yuv" || { echo "mismatch $b" >&2; exit 1; }
    echo "$b ok $(python3 -I -c 'import sys,zlib;print("%08x"%zlib.crc32(open(sys.argv[1],"rb").read()))' "$D/ref.yuv") $(stat -c %s "$D/ref.yuv")"
  else
    echo "$b refused $rc"
  fi
done
