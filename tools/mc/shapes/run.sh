#!/usr/bin/env bash
# tools/mc/shapes/run.sh -- every block state of the modelled families (fence, gate, wall, door, trapdoor, bed, chest, pane, bars, anvil, cauldron, hopper, farmland, path, soul sand, mud, honey) + the 141 table blocks (candles, bell, chain, lanterns, dripleaf, ladder, cactus, pots, heads, piston ...) vs minecraft-data
# blockCollisionShapes 1.20 (exact 1/16 voxel union). Counter-check: a damaged lib (fence arm 1/16 too short) must be detected.
# Usage: bash tools/mc/shapes/run.sh [firnc-binary]     (data dir: MCDATA, default the minecraft-data of tools/mcserver)
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
MCDATA=${MCDATA:-/root/firn/tools/mcserver/node_modules/minecraft-data/minecraft-data/data/pc/1.20}
[ -f "$MCDATA/blockCollisionShapes.json" ] || { echo "  FAIL  no minecraft-data at $MCDATA"; exit 1; }
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
run() { # libdir out
    FIRNLIB="$1" $FIRNC -o "$W/dump" tools/mc/shapes/dump.fi 2>"$W/err" || { head -10 "$W/err"; return 2; }
    "$W/dump" > "$2" || return 2
    python3 -I tools/mc/shapes/check.py "$2" "$MCDATA"
}
run "$(pwd)/lib" "$W/good.txt"; ERR=$?
# the generated table must be exactly what gen_tab.py produces from the data
python3 -I tools/mc/shapes/gen_tab.py "$MCDATA" "$W/regen.fi" >/dev/null && cmp -s "$W/regen.fi" lib/mc/mc_shape_data.fi && echo "  ok    mc_shape_data.fi is up to date" || { echo "  FAIL  lib/mc/mc_shape_data.fi differs from gen_tab.py output"; ERR=1; }
# counter-checks: every damaged lib must be detected (file|sed expression|label)
MUT=(
 "mc_math|s/arm_box(o, nth_dir(k - K_FENCE, i - 1), 0.375, 0.625, 0.625, 1.5)/arm_box(o, nth_dir(k - K_FENCE, i - 1), 0.375, 0.625, 0.3125, 1.5)/|fence arm"
 "mc_math|s/shape_set(o, 0.4375, 0.0, 0.0, 0.5625, 1.0, 0.5625)/shape_set(o, 0.4375, 0.0, 0.0, 0.5625, 1.0, 0.5)/|pane north arm"
 "mc_math|s/\"cacoeoeedmfn/\"dacoeoeedmfn/|anvil table"
 "mc_math|s/if k == K_HOPPER { return 13 }/if k == K_HOPPER { return 12 }/|hopper box count"
 "mc_math|s/\"aaacqeaamcqq/\"aaacqeaamcqr/|cauldron table"
 "mc_math|s/tbl16(o, \"babppp\", 0)/tbl16(o, \"babppo\", 0)/|honey box"
 "mc_shapes|s/mc_math.K_LOW + 14/mc_math.K_LOW + 13/|soul sand / mud height"
 "mc_shapes|s/if f >= 2 { return mc_math.SHAPE_BASE + mc_math.K_ANVIL + 1 }/if f >= 3 { return mc_math.SHAPE_BASE + mc_math.K_ANVIL + 1 }/|anvil facing"
 "mc_shape_data|/^fn boxes/s/return \"./return \"z/|table box coordinate"
 "mc_shape_data|/^fn states/s/return \"\(.\)./return \"\1z/|table state shape index"
 "mc_shape_data|/^fn names/s/bell;/belx;/|table block name"
 "mc_shape_data|/^fn poolidx/s/return \"\(...\)./return \"\1z/|table pool box count"
 "mc_shapes|s/if n != next - first { return 0 }/if n != next - first + 100 { return 0 }/|table state count guard"
 "mc_math|s/\* 0.03125, ((b26_at(t, b + 2, 2)/* 0.0625, ((b26_at(t, b + 2, 2)/|table x0 scale"
 "mc_math|s/(j as usize) \* 4 + 3) as i32) - 97/(j as usize) * 4 + 3) as i32) - 96/|table box count"
)
for m in "${MUT[@]}"; do
    IFS='|' read -r mf expr label <<<"$m"
    rm -rf "$W/badlib"; mkdir -p "$W/badlib"; cp -r lib/. "$W/badlib/"
    sed -i "$expr" "$W/badlib/mc/$mf.fi"
    if cmp -s "lib/mc/$mf.fi" "$W/badlib/mc/$mf.fi"; then echo "  FAIL  counter-check '$label': sed did not change anything"; ERR=1
    elif run "$W/badlib" "$W/bad.txt" >/dev/null 2>&1; then echo "  FAIL  counter-check: damaged $label NOT detected"; ERR=1
    else echo "  ok    counter-check: damaged $label detected"; fi
done
[ "$ERR" = 0 ] && echo "RESULT mc-shapes: ok" || echo "RESULT mc-shapes: FAIL"
exit "$ERR"
