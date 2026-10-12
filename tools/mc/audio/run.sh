#!/usr/bin/env bash
# tools/mc/audio/run.sh -- lib/mc/mc_audio.fi: pure spatial math (attenuation, Minecraft gain, pan, pan law) bit for bit
# against numpy float32 (tools/mc/audio/check.py), built in 3 stages (release-fast, --no-opt, dev-fast: byte-identical
# dumps), then the functional test tests/2766_mc_audio.fi (synthetic sounds through the real engine).
#
# Usage: bash tools/mc/audio/run.sh [firnc-binary]      (MC_AUDIO_SEEDS, MC_AUDIO_CASES)
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
SEEDS=${MC_AUDIO_SEEDS:-"1 2 3"}
CASES=${MC_AUDIO_CASES:-400}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/dump" tools/mc/audio/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    for seed in $SEEDS; do
        if ! "$W/dump" "$seed" "$CASES" > "$W/dump.$name.$seed" 2>"$W/stderr"; then
            echo "  FAIL  $name seed $seed: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
        fi
        if [ "$name" = "release-fast" ]; then
            python3 -I tools/mc/audio/check.py "$W/dump.$name.$seed" > "$W/out.txt" 2>&1; rc=$?
            if [ $rc -ne 0 ]; then
                echo "  FAIL  seed $seed:"; head -20 "$W/out.txt"; ERR=1
            else
                echo "  ok    seed $seed: $(grep -c . "$W/dump.$name.$seed") lines agree with the numpy reference"
                [ "$seed" = "${SEEDS%% *}" ] && sed -n '/cases:/p' "$W/out.txt"
            fi
        else
            cmp -s "$W/dump.$name.$seed" "$W/dump.release-fast.$seed" || { echo "  FAIL  $name seed $seed differs from release-fast"; ERR=1; }
        fi
    done
done
if $FIRNC -o "$W/t" tests/2766_mc_audio.fi 2>"$W/err"; then
    if "$W/t" > "$W/t.out" 2>&1; then
        echo "  ok    tests/2766_mc_audio.fi: $(grep -c '^ok' "$W/t.out") checks"
    else
        echo "  FAIL  tests/2766_mc_audio.fi"; grep -v '^ok' "$W/t.out" | head -20; ERR=1
    fi
else
    echo "  FAIL  tests/2766_mc_audio.fi does not compile"; head -10 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-audio: ok" || echo "RESULT mc-audio: FAIL"
exit $ERR
