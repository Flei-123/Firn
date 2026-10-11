#!/usr/bin/env bash
# tools/mc/entity/run.sh -- cross-check of lib/mc/mc_entity.fi against an independent reference.
#
# For every version of tools/mcgen/versions.json (10 releases):
#   1. gen.cjs builds a random stream of entity packets (spawn / move / look / teleport / sync / velocity /
#      destroy / player info+remove, unknown ids, respawns) from the version's protocol.json, serializes it with
#      the protodef serializer (node-minecraft-protocol) and computes the EXPECTED state with
#      tools/mcclient/ref_entities.cjs from the same parameter objects.
#   2. feed.fi (Firn) reads the serialized bodies with lib/mc/mc_entity.fi and prints entity_dump.
#   3. Both texts must be byte identical (positions/angles as raw f64 bits).
#   4. Counter-check: the same stream with the spawn packets fed under the wrong packet id must DIFFER
#      (otherwise the comparison could not fail).
# Also prints the throughput (packets/s). Run from any directory; MC_ENTITY_COUNT / MC_ENTITY_SEEDS tune it.
#
# The real-server proof (vanilla 1.20.1 / 1.8.9 / 1.21.4 against node-minecraft-protocol) is
# `MC_VERSION=<rel> bash tools/mcclient/run.sh <client>` -- see docs/mc/entity.md.
set -uo pipefail
cd "$(dirname "$0")/../../.."
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
COUNT=${MC_ENTITY_COUNT:-3000}
SEEDS=${MC_ENTITY_SEEDS:-"1 2"}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
if ! $FIRNC -o "$W/feed" tools/mc/entity/feed.fi 2>"$W/err"; then
    echo "  FAIL  feed.fi does not compile"; head -10 "$W/err"; echo "RESULT mc-entity: FAIL"; exit 1
fi
RELS=$(node -e 'console.log(Object.keys(JSON.parse(require("fs").readFileSync("tools/mcgen/versions.json","utf8")).versions).join(" "))')
for rel in $RELS; do
    for seed in $SEEDS; do
        D="$W/$rel.$seed"
        if ! node tools/mc/entity/gen.cjs "$rel" "$D" "$COUNT" "$seed" > "$W/gen.out" 2>"$W/gen.err"; then
            echo "  FAIL  $rel seed $seed: generator failed"; head -5 "$W/gen.err"; ERR=$((ERR+1)); continue
        fi
        proto=$(cat "$D/protocol")
        "$W/feed" "$proto" "$D/script.txt" > "$D/got.all" 2>"$W/feed.err" || { echo "  FAIL  $rel seed $seed: feed crashed"; head -3 "$W/feed.err"; ERR=$((ERR+1)); continue; }
        grep -v -E '^(counters|rate) ' "$D/got.all" > "$D/got.txt"
        if cmp -s "$D/got.txt" "$D/expected.txt"; then
            named=$(grep -c ' name=[^-]' "$D/got.txt")
            echo "  ok    $rel (protocol $proto) seed $seed: $(head -1 "$D/got.txt" | cut -d' ' -f2) entities equal, $named named; $(cut -d' ' -f4- "$W/gen.out" | cut -c1-90)"
            [ "$seed" = "${SEEDS%% *}" ] && { grep '^rate' "$D/got.all" | sed "s/^/        $rel /"; }
        else
            echo "  FAIL  $rel seed $seed: entity dump differs from the reference (< reference, > Firn):"
            diff "$D/expected.txt" "$D/got.txt" | head -6 | sed 's/^/        /'
            ERR=$((ERR+1)); continue
        fi
        if [ "$seed" = "${SEEDS%% *}" ]; then
            "$W/feed" "$proto" "$D/script.txt" --break 2>/dev/null | grep -v -E '^(counters|rate) ' > "$D/broken.txt"
            if cmp -s "$D/broken.txt" "$D/expected.txt"; then
                echo "  FAIL  $rel: counter-check did not change the dump (the comparison cannot fail)"; ERR=$((ERR+1))
            fi
        fi
    done
done
[ $ERR -eq 0 ] && echo "  ok    counter-check: a wrong spawn packet id always changes the dump"
if [ $ERR -eq 0 ]; then echo "RESULT mc-entity: ok"; else echo "RESULT mc-entity: FAIL ($ERR)"; fi
exit $ERR
