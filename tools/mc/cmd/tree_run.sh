#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
REPORTS=${MC_REPORTS:-/root/.w5}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt"; do
    name=${stage:-release}
    if ! $FIRNC $stage -o "$W/dump" tools/mc/cmd/tree_dump.fi 2>"$W/cerr"; then
        echo "  FAIL  compile ($name)"; head -8 "$W/cerr"; ERR=1; continue
    fi
    for ver in 1.20.1 26.1 26.3; do
        f="$REPORTS/rep-$ver/out/reports/commands.json"
        [ -f "$f" ] || { echo "  skip  $ver (no report)"; continue; }
        python3 -I tools/mc/cmd/ref_dump.py "$f" > "$W/ref.txt"
        "$W/dump" "$f" > "$W/got.txt" 2>"$W/rerr" || { echo "  FAIL  run $ver ($name)"; head -3 "$W/rerr"; ERR=1; continue; }
        if cmp -s "$W/ref.txt" "$W/got.txt"; then
            echo "  ok    tree $ver $name: $(wc -l < "$W/ref.txt") nodes identical"
        else
            echo "  FAIL  tree $ver $name: $(diff "$W/ref.txt" "$W/got.txt" | grep -c '^<') lines differ"
            diff "$W/ref.txt" "$W/got.txt" | head -6 | cut -c1-200
            ERR=1
        fi
    done
done
exit $ERR
