#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
fail=0
check() {
    local name=$1 want=$2 got=$3
    if [ "$want" = "$got" ]; then
        echo "ok   $name"
    else
        echo "FAIL $name: want '$want' got '$got'"
        fail=$((fail + 1))
    fi
}
build() {
    "$FIRNC" "$@" -o "$WORK/out" >"$WORK/log" 2>&1
    echo $?
}
D=$ROOT/tools/safety/unsafe
check "outside, mode default" 0 "$(build "$D/outside.fi")"
check "outside, mode off" 0 "$(build --unsafe=off "$D/outside.fi")"
check "outside, mode warn builds" 0 "$(build --unsafe=warn "$D/outside.fi")"
check "outside, mode warn reports cast" 1 "$(grep -c 'warning: a raw pointer cast' "$WORK/log")"
check "outside, mode warn reports deref" 1 "$(grep -c 'warning: dereferencing a raw pointer' "$WORK/log")"
check "outside, mode error fails" 1 "$(build --unsafe=error "$D/outside.fi")"
check "outside, mode error reports 2" 2 "$(grep -c '^error: ' "$WORK/log")"
check "outside, scope elsewhere" 0 "$(build --unsafe=error --unsafe-scope=/nowhere/ "$D/outside.fi")"
check "outside, scope here" 1 "$(build --unsafe=error --unsafe-scope=safety/unsafe/ "$D/outside.fi")"
check "wrapped, mode error" 0 "$(build --unsafe=error "$D/wrapped.fi")"
check "wrapped, runs" 10 "$("$WORK/out" >/dev/null 2>&1; echo $?)"
check "bad mode rejected" 2 "$(build --unsafe=maybe "$D/wrapped.fi")"
while read -r rel; do
    mod=${rel#lib/}
    mod=${mod%.fi}
    printf 'import %s\nfn main() -> i32 {\n    return 0\n}\n' "${mod//\//.}" >"$WORK/m.fi"
    check "migrated $rel clean" 0 "$(build --unsafe=error --unsafe-scope="$rel" "$WORK/m.fi")"
done <"$ROOT/tools/safety/unsafe_migrated.txt"
[ "$fail" -eq 0 ] || { echo "$fail failed"; exit 1; }
echo "all ok"
