#!/usr/bin/env bash
# tools/mc/assets/run.sh -- mc_assets test (self-built test packs: priority, fallback, resample, broken ZIPs).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/t" tests/2812_mc_assets.fi 2>"$W/err"; then
        echo "  FAIL  $name: does not compile"; head -15 "$W/err"; ERR=1; continue
    fi
    "$W/t"; rc=$?
    [ $rc -eq 0 ] && echo "  ok    $name" || { echo "  FAIL  $name: exit $rc"; ERR=1; }
done
[ $ERR -eq 0 ] && echo "RESULT mc-assets: ok" || echo "RESULT mc-assets: FAIL"
exit $ERR
