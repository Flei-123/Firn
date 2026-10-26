#!/usr/bin/env bash
# Usage: bash tools/cache/measure_wasm.sh <cwd> <source.fi> [extra firnc flags]
# Prints wall/user/RSS/size for: uncached, cold (miss), hit, one-file change.
set -uo pipefail
FIRNC=${FIRNC:-$(cd "$(dirname "$0")/../.." && pwd)/compiler/target/release/firnc}
cd "$1"
SRC=$2
shift 2
C=$(mktemp -d)
D=$(mktemp -d)
PROBE="$(dirname "$SRC")/zz_probe_$$.fi"
trap 'rm -rf "$C" "$D" "$PROBE"' EXIT
export FIRN_CACHE_DIR="$C"
A="--opt-level=release-safe --target=wasm32-browser $*"
run() {
    local tag=$1
    shift
    /usr/bin/time -f "$tag %e s wall %U s user %M KB" "$@" 2>&1 >/dev/null | tail -1
}
run uncached "$FIRNC" $A --no-cache -o "$D/nc.wasm" "$SRC"
run cold "$FIRNC" $A -o "$D/cold.wasm" "$SRC"
run hit "$FIRNC" $A -o "$D/hit.wasm" "$SRC"
cp "$SRC" "$PROBE"
printf '\n' >> "$PROBE"
run change "$FIRNC" $A -o "$D/chg.wasm" "$PROBE"
cmp -s "$D/nc.wasm" "$D/cold.wasm" && cmp -s "$D/nc.wasm" "$D/hit.wasm" && echo "bytes identical: yes ($(stat -c %s "$D/nc.wasm") B)" || echo "bytes identical: NO"
