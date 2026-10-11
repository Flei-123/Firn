#!/usr/bin/env bash
# tools/mc/blend/run.sh -- tests/2756_mc_blend.fi in four build stages + the speed bench (SIMD vs scalar).
# Usage: bash tools/mc/blend/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast" "--opt-level=release-safe" "--asm-intern"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/t" tests/2756_mc_blend.fi 2>"$W/err"; then
        echo "  FAIL  $name: does not compile"; head -5 "$W/err"; ERR=1; continue
    fi
    "$W/t"; rc=$?
    if [ $rc -eq 0 ]; then echo "  ok    $name: SIMD == scalar"; else echo "  FAIL  $name: exit $rc"; ERR=1; fi
done
if $FIRNC -o "$W/bench" tools/mc/blend/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc/blend: ok"
exit $ERR
