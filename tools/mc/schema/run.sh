#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/schema/run.sh -- generator + schema walker, for EVERY generated game version.
#
#   1. tools/mcgen/gen_protocol.py regenerates lib/mc/mc_sem.fi, mc_versions.fi and all
#      lib/mc/vNNN/mc_proto_vNNN.fi into a temp dir; they must be byte-identical to the
#      committed files (the committed tables are never edited by hand);
#   2. per version: tools/mcgen/gen_fixtures.cjs writes example bodies of EVERY packet with
#      the REFERENCE serializer (node-minecraft-protocol / protodef, needs node + the
#      node_modules of tools/mcserver; without them the fixture part is SKIPPED, loudly);
#   3. per version: tools/mc/schema/check.fi (compiled in four build stages) demands exact
#      consumption of every fixture, rejection of every truncated / extended body, and no
#      crash on mutated or random input.
# The same walker and the same check program run for every version in versions.json (1.8.9 .. 26.1):
# that is the modularity proof of docs/MC-VERSIONS.md (adding a version needs no walker change).
# Usage: bash tools/mc/schema/run.sh [FIRNC] [only-protocol]
set -u
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
FIRNC=${1:-${FIRNC:-$ROOT/compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
ONLY=${2:-}
NM=${MC_NODE_MODULES:-/root/firn/tools/mcserver/node_modules}
MCDATA=$NM/minecraft-data/minecraft-data/data
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
rc=0
python3 "$ROOT/tools/mcgen/gen_protocol.py" --all "$MCDATA" "$D/gen" all 2>"$D/gen.log" || { cat "$D/gen.log"; echo "FAIL generator"; exit 1; }
for f in mc_sem.fi mc_versions.fi $(cd "$D/gen" && ls v*/mc_proto_v*.fi); do
    if cmp -s "$D/gen/$f" "$ROOT/lib/mc/$f"; then :; else echo "FAIL: lib/mc/$f differs from the generator output"; rc=1; fi
done
[ $rc -eq 0 ] && echo "  ok: all generated files are up to date ($(grep -c . "$D/gen.log") versions)"
FIRNLIB=$ROOT/lib "$FIRNC" -o "$D/basic" "$ROOT/tests/2650_mc_schema_basic.fi" >"$D/cc.log" 2>&1 || { cat "$D/cc.log"; echo "FAIL compile basic"; exit 1; }
"$D/basic" || { echo "FAIL basic exit $?"; rc=1; }
if ! command -v node >/dev/null 2>&1 || [ ! -d "$NM/protodef" ]; then
    echo "  SKIP: node or $NM/protodef missing - fixtures not checked"
    exit $rc
fi
python3 - "$ROOT/tools/mcgen/versions.json" > "$D/versions.txt" <<'PY'
import json, sys
for k, v in json.load(open(sys.argv[1]))["versions"].items():
    print(k, v.get("nmp", v["data"]), v["protocol"])
PY
# the check program does not depend on the game version: compile it once per build stage
MODES=("" "--no-opt" "--opt-level=dev-fast" "--opt-level=release-safe")
mi=0
for mode in "${MODES[@]}"; do
    FIRNLIB=$ROOT/lib "$FIRNC" $mode -o "$D/sc$mi" "$ROOT/tools/mc/schema/check.fi" >"$D/cc$mi.log" 2>&1 || { cat "$D/cc$mi.log"; echo "FAIL compile check [$mode]"; rc=1; }
    mi=$((mi + 1))
done
while read -r key nmpv proto; do
    [ -n "$ONLY" ] && [ "$ONLY" != "$proto" ] && continue
    NODE_PATH=$NM node "$ROOT/tools/mcgen/gen_fixtures.cjs" "$D/fx$proto" "$nmpv" 4 >"$D/fx.log" 2>&1 || { cat "$D/fx.log"; echo "FAIL fixtures $key"; rc=1; continue; }
    tail -1 "$D/fx.log" | sed "s/^/  [$key] /"
    mi=0
    for mode in "${MODES[@]}"; do
        if [ -x "$D/sc$mi" ]; then
            if "$D/sc$mi" "$D/fx$proto" 3 "$proto" >"$D/out.log" 2>&1; then
                echo "  ok [$key ${mode:-release-fast}]: $(grep -E '^check (fixtures|pass_exact)' "$D/out.log" | tr '\n' ' ')"
            else
                head -20 "$D/out.log"; echo "FAIL check [$key ${mode:-release-fast}]"; rc=1
            fi
            [ -z "$mode" ] && grep -E '^metric' "$D/out.log" | sed "s/^/    [$key] /"
        fi
        mi=$((mi + 1))
    done
    rm -rf "$D/fx$proto"
done < "$D/versions.txt"
[ $rc -eq 0 ] && echo "RESULT mc-schema: ok" || echo "RESULT mc-schema: FAIL"
exit $rc
