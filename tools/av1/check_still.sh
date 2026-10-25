#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level="${AV1_OPT:-release-safe}" -o "$D/yuvdump" tools/av1/yuvdump.fi >"$D/build.log" 2>&1 || { tail -5 "$D/build.log"; echo "check_still: build FAILED"; exit 1; }
ok=0
refused=0
bad=0
for F in tests/data/av1/${1:-*}.avif; do
  b=$(basename "$F" .avif)
  "$D/yuvdump" <"$F" >"$D/out.yuv"
  rc=$?
  if [ $rc -ne 0 ]; then
    refused=$((refused + 1))
    echo "refused $b exit=$rc"
    continue
  fi
  ffmpeg -v error -y -i "$F" -f rawvideo "$D/ref.yuv" 2>/dev/null || { echo "ffmpeg FAILED on $b"; bad=$((bad + 1)); continue; }
  if cmp -s "$D/out.yuv" "$D/ref.yuv"; then
    ok=$((ok + 1))
  else
    echo "MISMATCH $b"
    bad=$((bad + 1))
  fi
done
echo "check_still: exact=$ok refused=$refused mismatch=$bad"
[ $bad -eq 0 ]
