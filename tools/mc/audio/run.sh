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
# real server packets (fixtures recorded by tools/mc/audio/sounds_live.sh from vanilla 1.20.1 / 1.21.4): mc_audio's decode must equal
# node-minecraft-protocol's parse (check_wire.py). tests/2792_mc_audio_wire.fi covers the generated id tables + dispatch.
if $FIRNC -o "$W/pkts" tools/mc/audio/sound_pkts.fi 2>"$W/err"; then
    for pair in "1.20.1 763" "1.21.4 769"; do
        set -- $pair; fx="tools/mc/audio/fixtures/sounds_${1//./_}.jsonl"
        python3 -I -c 'import json,sys
for l in open(sys.argv[2]):
    j=json.loads(l)
    if j["hex"]: print(sys.argv[1], j["pkt"], j["hex"])' "$2" "$fx" | "$W/pkts" > "$W/wire.$1" 2>"$W/stderr"
        python3 -I tools/mc/audio/check_wire.py "$1" "$fx" "$W/wire.$1" || ERR=1
    done
else
    echo "  FAIL  sound_pkts.fi does not compile"; head -10 "$W/err"; ERR=1
fi
if $FIRNC -o "$W/t2" tests/2792_mc_audio_wire.fi 2>"$W/err" && "$W/t2" > "$W/t2.out" 2>&1; then
    echo "  ok    tests/2792_mc_audio_wire.fi: $(grep -c '^ok' "$W/t2.out") checks"
else
    echo "  FAIL  tests/2792_mc_audio_wire.fi"; head -10 "$W/err"; grep -v '^ok' "$W/t2.out" | head; ERR=1
fi
# synth bank vs sound id table (1.20.1): report gaps, tier 1 = heard constantly (heuristic ranking, see coverage.py); baseline = 650
python3 -I tools/mc/audio/coverage.py lib/mc/v763/mc_snd_v763.fi tools/sounds/mc.json --top 0 --check --allow 700 | sed 's/^/  /' || ERR=1
[ $ERR -eq 0 ] && echo "RESULT mc-audio: ok" || echo "RESULT mc-audio: FAIL"
exit $ERR
