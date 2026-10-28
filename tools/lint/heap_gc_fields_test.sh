#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
L=tools/lint/heap_gc_fields.py
C=tools/lint/cases
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
rc=0
expect() {
    local name=$1 want=$2
    mkdir -p "$D/$name"
    cp "$C/$name.fi" "$D/$name/"
    python3 "$L" "$D/$name" >"$D/$name.out" 2>&1
    local got=$?
    if [ "$got" != "$want" ]; then
        echo "FAIL $name: exit $got, wanted $want"
        cat "$D/$name.out"
        rc=1
    else
        echo "ok $name"
    fi
}
expect bad 1
expect good 0
expect bad_generic 1
exit $rc
