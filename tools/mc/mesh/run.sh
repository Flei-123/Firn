#!/usr/bin/env bash
# tools/mc/mesh/run.sh -- mc_mesh against a BRUTE FORCE checker written independently in python
# (its own face visibility rule, its own ambient occlusion):
#   * 11 world shapes (empty, full, checkerboard, random 20/50/80 % with 9 block kinds, unknown shell,
#     terrain, single block, glass blob, layers) x 8 flag combinations (naive / greedy, AO on / off,
#     cull-same on / off) = 128 meshes: the quads cover EXACTLY the visible faces once, tile and the four
#     corner AO values of every cell equal the quad's, naive = one quad per face, greedy <= naive, the
#     index buffer has the right diagonal and counter-clockwise winding;
#   * benchmark: sections per second (naive and greedy, with AO).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
$FIRNC -o "$W/prog" tools/mc/mesh/prog.fi 2>"$W/err" || { echo "  FAIL  compile"; head -5 "$W/err"; exit 1; }
if python3 -I tools/mc/mesh/verify.py "$W/prog" "$W" > "$W/verify.txt" 2>&1; then
    tail -4 "$W/verify.txt"
else
    echo "  FAIL  brute force check"; grep -E 'FAIL|Error|assert' "$W/verify.txt" | head -5; ERR=1
fi
python3 -I tools/mc/mesh/verify.py emit random50 3 "$W/b50.bin"
python3 -I tools/mc/mesh/verify.py emit terrain 3 "$W/terrain.bin"
for f in 2 3; do
    for w in b50 terrain; do
        echo "  metric $w flags=$f: $("$W/prog" bench "$W/$w.bin" $f 300)"
    done
done
# counter-check: a damaged mesher must be caught -- patch the AO rule (darker corner) and expect a failure
mkdir -p "$W/badlib" && cp -r lib/. "$W/badlib/" && sed -i 's/return 3 as u32 - (s1 + s2 + c)/return 2 as u32 - (s1 + s2 + c) + 1 as u32 - 1 as u32 + c/' "$W/badlib/mc/mc_mesh.fi"
if FIRNLIB="$W/badlib" $FIRNC -o "$W/prog_bad" tools/mc/mesh/prog.fi 2>/dev/null; then
    if python3 -I tools/mc/mesh/verify.py "$W/prog_bad" "$W" > "$W/bad.txt" 2>&1; then
        echo "  FAIL  counter-check: a damaged AO rule passed"; ERR=1
    else
        echo "  ok    counter-check: a damaged AO rule is detected"
    fi
else
    echo "  FAIL  counter-check build"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-mesh: ok" || echo "RESULT mc-mesh: FAIL"
exit $ERR
