#!/bin/bash
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/av1hdr.fi" -o "$D/av1hdr"
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/av1symtest.fi" -o "$D/symtest"
bash "$ROOT/tools/media_video/gen_av1_clips.sh" "$D/clips" >/dev/null 2>&1 || true
find "$D/clips" -name '*.obu' -size 0 -delete
python3 -I "$ROOT/tools/media_video/av1hdr_cross.py" "$D/av1hdr" "$D"/clips/*.obu
python3 -I "$ROOT/tools/media_video/av1symtest.py" "$D/symtest" 60
FIRNLIB=$ROOT/lib ${FIRNC:-firnc} "$ROOT/tools/media_video/av1coeftest.fi" -o "$D/coeftest"
python3 -I "$ROOT/tools/media_video/av1coeftest.py" "$D/coeftest" 60
FUZZ_OUT="$D" python3 -I "$ROOT/tools/media_video/fuzz_av1.py" "$D/clips" "$D/av1hdr" ${FUZZ_RUNS:-1500}
python3 -I "$ROOT/tools/media_video/av1cdf_selftest.py" "${FIRNC:-firnc}" ${AV1_LIBAOM_SRC:+--src "$AV1_LIBAOM_SRC"}
