#!/usr/bin/env bash
# tools/mc/shapes/run.sh -- every block state of the modelled families (fence, gate, wall, door, trapdoor, bed, chest) vs minecraft-data
# blockCollisionShapes 1.20 (exact 1/16 voxel union). Counter-check: a damaged lib (fence arm 1/16 too short) must be detected.
# Usage: bash tools/mc/shapes/run.sh [firnc-binary]     (data dir: MCDATA, default the minecraft-data of tools/mcserver)
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
MCDATA=${MCDATA:-/root/firn-vt/tools/mcserver/node_modules/minecraft-data/minecraft-data/data/pc/1.20}
[ -f "$MCDATA/blockCollisionShapes.json" ] || { echo "  FAIL  no minecraft-data at $MCDATA"; exit 1; }
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
run() { # libdir out
    FIRNLIB="$1" $FIRNC -o "$W/dump" tools/mc/shapes/dump.fi 2>"$W/err" || { head -10 "$W/err"; return 2; }
    "$W/dump" > "$2" || return 2
    python3 -I tools/mc/shapes/check.py "$2" "$MCDATA"
}
run "$(pwd)/lib" "$W/good.txt"; ERR=$?
# counter-check
mkdir -p "$W/badlib"; cp -r lib/. "$W/badlib/"
sed -i 's/arm_box(o, nth_dir(k - K_FENCE, i - 1), 0.375, 0.625, 0.625, 1.5)/arm_box(o, nth_dir(k - K_FENCE, i - 1), 0.375, 0.625, 0.3125, 1.5)/' "$W/badlib/mc/mc_math.fi"
if cmp -s lib/mc/mc_math.fi "$W/badlib/mc/mc_math.fi"; then echo "  FAIL  counter-check sed did not change anything"; ERR=1
elif run "$W/badlib" "$W/bad.txt" >/dev/null 2>&1; then echo "  FAIL  counter-check: damaged fence arm NOT detected"; ERR=1
else echo "  ok    counter-check: damaged fence arm detected"; fi
[ "$ERR" = 0 ] && echo "RESULT mc-shapes: ok" || echo "RESULT mc-shapes: FAIL"
exit "$ERR"
