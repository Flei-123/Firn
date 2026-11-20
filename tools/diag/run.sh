#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB=$PWD/lib
FIRNC=${FIRNC:-compiler/target/release/firnc}
FIRNC1=${FIRNC1:-}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-diag.XXXXXX")
trap 'rm -rf "$W"' EXIT
if [ -z "$FIRNC1" ]; then
    FIRNC1=$W/firnc1
    "$FIRNC" bin/firnc1.fi -o "$FIRNC1" > "$W/build.log" 2>&1 || { cat "$W/build.log"; exit 1; }
fi
PASS=0
FAIL=0
GAPS=0
for f in tools/diag/cases/*.fi; do
    name=$(basename "$f" .fi)
    want=$(sed -n 's#^// expect_help: ##p' "$f" | head -1)
    c1=$(sed -n 's#^// expect_c1: ##p' "$f" | head -1)
    "$FIRNC" "$f" -o "$W/out0" > "$W/e0" 2>&1
    "$FIRNC1" "$f" -o "$W/out1" > "$W/e1" 2>&1
    if grep -qF "= help: $want" "$W/e0"; then
        PASS=$((PASS + 1)); printf '  %-28s firnc0 ok\n' "$name"
    else
        FAIL=$((FAIL + 1)); printf '  %-28s firnc0 FAIL (no help: %s)\n' "$name" "$want"
    fi
    if [ "$c1" = none ]; then
        if grep -qF "= help: $want" "$W/e1"; then
            FAIL=$((FAIL + 1)); printf '  %-28s firnc1 now has the help: drop expect_c1\n' "$name"
        else
            GAPS=$((GAPS + 1)); printf '  %-28s firnc1 gap (known)\n' "$name"
        fi
    elif grep -qF "= help: $want" "$W/e1"; then
        PASS=$((PASS + 1)); printf '  %-28s firnc1 ok\n' "$name"
    else
        FAIL=$((FAIL + 1)); printf '  %-28s firnc1 FAIL\n' "$name"
    fi
done
echo "diag helps: $PASS ok, $FAIL errors, $GAPS known firnc1 gaps"
[ "$FAIL" -eq 0 ]
