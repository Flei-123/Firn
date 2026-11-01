#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} -o "$D/mfmv_dump" "$ROOT/tools/media_video/mfmv_dump.fi"
"$D/mfmv_dump" "$D/got.txt"
python3 -I "$ROOT/tools/media_video/av1_mfmv_ref.py" "$D/want.txt"
if cmp -s "$D/got.txt" "$D/want.txt"; then
  echo "ok mfmv $(wc -l < "$D/got.txt") lines, valid cells: $(awk '{s+=$3} END{print s}' "$D/got.txt")"
else
  diff "$D/got.txt" "$D/want.txt" | head -10
  exit 1
fi
