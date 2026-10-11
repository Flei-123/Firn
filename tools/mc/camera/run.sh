#!/usr/bin/env bash
# tools/mc/camera/run.sh -- cross-check of lib/mc/mc_camera.fi against an independent numpy
# reference (tools/mc/camera/check.py): basis from rotation matrices, view / projection matrices,
# frustum corners and planes, inverse view-projection, pick rays by unprojection, world->pixel,
# boxes and 16^3 sections against the frustum by an exact separating-axis test. The dump is built
# in 3 stages; all three must write byte-identical output. Prints the time of the hot paths.
#
# Usage: bash tools/mc/camera/run.sh [firnc-binary]      (MC_CAMERA_SEEDS, MC_CAMERA_CASES)
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
SEEDS=${MC_CAMERA_SEEDS:-"1 2 3"}
CASES=${MC_CAMERA_CASES:-40}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/dump" tools/mc/camera/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    for seed in $SEEDS; do
        if ! "$W/dump" "$seed" "$CASES" > "$W/dump.$name.$seed" 2>"$W/stderr"; then
            echo "  FAIL  $name seed $seed: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
        fi
        if [ "$name" = "release-fast" ]; then
            python3 -I tools/mc/camera/check.py "$W/dump.$name.$seed" > "$W/out.txt" 2>&1; rc=$?
            if [ $rc -ne 0 ]; then
                echo "  FAIL  seed $seed:"; head -30 "$W/out.txt"; ERR=1
            else
                echo "  ok    seed $seed: $(grep -c . "$W/dump.$name.$seed") lines agree with the numpy reference"
                [ "$seed" = "${SEEDS%% *}" ] && sed -n '/cases:/p;/counter-check/p' "$W/out.txt"
            fi
        else
            cmp -s "$W/dump.$name.$seed" "$W/dump.release-fast.$seed" || { echo "  FAIL  $name seed $seed differs from release-fast"; ERR=1; }
        fi
    done
done
if $FIRNC -o "$W/bench" tools/mc/camera/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-camera: ok" || echo "RESULT mc-camera: FAIL"
exit $ERR
