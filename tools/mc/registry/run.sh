#!/usr/bin/env bash
# tools/mc/registry/run.sh -- mc_registry (generated tables + reader) against minecraft-data +
# prismarine-block, for every generated version: blocks, ALL block states with their
# properties, items, entities and biomes must print identically. Counter-check: one damaged
# line must be detected.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
NM=${MC_NODE_MODULES:-/root/firn/tools/mcserver/node_modules}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
command -v node >/dev/null && [ -d "$NM/prismarine-block" ] || { echo "SKIP: node or prismarine-block missing"; exit 0; }
GEN=$W/gen
python3 tools/mcgen/gen_protocol.py --all "$NM/minecraft-data/minecraft-data/data" "$GEN" all 2>/dev/null || { echo "FAIL generator"; exit 1; }
for f in $(cd "$GEN" && ls v*/mc_reg_v*.fi mc_sem.fi); do
    cmp -s "$GEN/$f" "lib/mc/$f" || { echo "  FAIL  lib/mc/$f differs from the generator output"; ERR=1; }
done
$FIRNC -o "$W/dump" tools/mc/registry/dump.fi 2>"$W/err" || { echo "FAIL compile"; head "$W/err"; exit 1; }
python3 - tools/mcgen/versions.json > "$W/versions.txt" <<'PY'
import json, sys
for k, v in json.load(open(sys.argv[1]))["versions"].items():
    print(k, v["nmp"], v["protocol"])
PY
while read -r key nmpv proto; do
    "$W/dump" "$proto" > "$W/got.txt" 2>"$W/err" || { echo "  FAIL  $key: dump exit $?"; ERR=1; continue; }
    NODE_PATH=$NM node tools/mc/registry/ref.cjs "$nmpv" "$proto" > "$W/want.txt" 2>"$W/err" || { echo "  FAIL  $key: reference failed"; head -3 "$W/err"; ERR=1; continue; }
    if cmp -s "$W/got.txt" "$W/want.txt"; then
        echo "  ok    $key: $(grep -c '^B ' "$W/got.txt") blocks, $(grep -c '^S ' "$W/got.txt") states, $(grep -c '^I ' "$W/got.txt") items, $(grep -c '^E ' "$W/got.txt") entities, $(grep -c '^M ' "$W/got.txt") biomes identical to minecraft-data/prismarine-block"
    else
        echo "  FAIL  $key: differs"; diff "$W/got.txt" "$W/want.txt" | head -6; ERR=1
    fi
done < "$W/versions.txt"
# counter-check
sed '2000s/^\(S [0-9]* \)/\1X/' "$W/want.txt" > "$W/bad.txt"
cmp -s "$W/got.txt" "$W/bad.txt" && { echo "  FAIL  counter-check: damaged reference equal"; ERR=1; } || echo "  ok    counter-check: a damaged reference is detected"
[ $ERR -eq 0 ] && echo "RESULT mc-registry: ok" || echo "RESULT mc-registry: FAIL"
exit $ERR
