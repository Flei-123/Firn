#!/usr/bin/env bash
# tools/mc/texture/run.sh -- mc_texture against an independent python re-derivation (check.py):
# the 1.20.1 state -> tile + opaque tables from minecraft-data blocks.json, plain-noise tiles pixel-exact,
# patterned tiles structurally; three build stages must write identical files. Prints build times.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
MCD=${MC_DATA:-/root/firn/tools/mcserver/node_modules/minecraft-data/minecraft-data/data}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/dump" tools/mc/texture/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    "$W/dump" "$W/atlas.$name" "$W/tiles.$name" > "$W/m.$name" || { echo "  FAIL  $name: dump crashed"; ERR=1; continue; }
    if [ "$name" = "release-fast" ]; then
        python3 -I tools/mc/texture/check.py "$W/atlas.$name" "$W/tiles.$name" "$MCD/pc/1.20/blocks.json" || ERR=1
        sed 's/^/  /' "$W/m.$name"
    else
        cmp -s "$W/atlas.$name" "$W/atlas.release-fast" && cmp -s "$W/tiles.$name" "$W/tiles.release-fast" || { echo "  FAIL  $name differs from release-fast"; ERR=1; }
    fi
done
[ $ERR -eq 0 ] && echo "RESULT mc-texture: ok" || echo "RESULT mc-texture: FAIL"
exit $ERR
