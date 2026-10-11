#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/encode/run.sh -- the schema driven WRITER (lib/mc/mc_encode.fi), for EVERY generated
# game version.
#
#   per version: tools/mcgen/gen_fixtures.cjs writes example bodies of EVERY packet (both
#   directions) with the REFERENCE serializer (node-minecraft-protocol / protodef; needs node + the
#   node_modules of tools/mcserver, otherwise the fixture part is SKIPPED, loudly);
#   tools/mc/encode/check.fi (compiled in four build stages) decodes each body into the lossless
#   value store (mc_schema.walk_values), encodes the store again (mc_encode.encode) and demands the
#   reference BYTES back, also with the store in reverse order; damaged stores must not crash.
#   The known answers (hand computed bodies, refusals) are tests/2651_mc_encode_basic.fi
#   (tools/mc/testmc.sh encode).
# Usage: bash tools/mc/encode/run.sh [FIRNC] [only-protocol]
set -u
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
FIRNC=${1:-${FIRNC:-$ROOT/compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
ONLY=${2:-}
NM=${MC_NODE_MODULES:-/root/firn/tools/mcserver/node_modules}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
rc=0
FIRNLIB=$ROOT/lib "$FIRNC" -o "$D/basic" "$ROOT/tests/2651_mc_encode_basic.fi" >"$D/cc.log" 2>&1 || { cat "$D/cc.log"; echo "FAIL compile basic"; exit 1; }
"$D/basic" || { echo "FAIL basic exit $?"; rc=1; }
if ! command -v node >/dev/null 2>&1 || [ ! -d "$NM/protodef" ]; then
    echo "  SKIP: node or $NM/protodef missing - fixtures not checked"
    [ $rc -eq 0 ] && echo "RESULT mc-encode: ok (fixtures skipped)" || echo "RESULT mc-encode: FAIL"
    exit $rc
fi
python3 - "$ROOT/tools/mcgen/versions.json" > "$D/versions.txt" <<'PY'
import json, sys
for k, v in json.load(open(sys.argv[1]))["versions"].items():
    print(k, v.get("nmp", v["data"]), v["protocol"])
PY
MODES=("" "--no-opt" "--opt-level=dev-fast" "--opt-level=release-safe")
mi=0
for mode in "${MODES[@]}"; do
    FIRNLIB=$ROOT/lib "$FIRNC" $mode -o "$D/sc$mi" "$ROOT/tools/mc/encode/check.fi" >"$D/cc$mi.log" 2>&1 || { cat "$D/cc$mi.log"; echo "FAIL compile check [$mode]"; rc=1; }
    mi=$((mi + 1))
done
while read -r key nmpv proto; do
    [ -n "$ONLY" ] && [ "$ONLY" != "$proto" ] && continue
    NODE_PATH=$NM node "$ROOT/tools/mcgen/gen_fixtures.cjs" "$D/fx$proto" "$nmpv" 4 >"$D/fx.log" 2>&1 || { cat "$D/fx.log"; echo "FAIL fixtures $key"; rc=1; continue; }
    tail -1 "$D/fx.log" | sed "s/^/  [$key] /"
    mi=0
    for mode in "${MODES[@]}"; do
        if [ -x "$D/sc$mi" ]; then
            if "$D/sc$mi" "$D/fx$proto" "$proto" >"$D/out.log" 2>&1; then
                echo "  ok [$key ${mode:-release-fast}]: $(grep -E '^check (fixtures_c2s|pass_exact|pass_c2s|reversed_fail|nocount_fail)' "$D/out.log" | sed 's/^check //' | tr '\n' ' ')"
            else
                head -40 "$D/out.log"; echo "FAIL check [$key ${mode:-release-fast}]"; rc=1
            fi
            [ -z "$mode" ] && grep -E '^metric' "$D/out.log" | sed "s/^/    [$key] /"
        fi
        mi=$((mi + 1))
    done
    rm -rf "$D/fx$proto"
done < "$D/versions.txt"
[ $rc -eq 0 ] && echo "RESULT mc-encode: ok" || echo "RESULT mc-encode: FAIL"
exit $rc
