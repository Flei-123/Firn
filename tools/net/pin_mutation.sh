#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Counter-check for tests/2869_async_closure_pin.fi: with the pin call
# removed the capturing closure must NOT survive (exit != 0). Without this the
# test could pass by accident (the conservative stack scan keeping it alive).
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
sed 's|^\(.*\)\*ud_slot = fnpin.pin(f as u64) //PIN|\1*ud_slot = 0|' tests/2869_async_closure_pin.fi > "$D/m.fi"
grep -q 'ud_slot = 0' "$D/m.fi" || { echo "FAIL mutation not applied"; exit 1; }
"$FIRNC" -o "$D/m" "$D/m.fi" 2>/dev/null || { echo "FAIL mutant does not build"; exit 1; }
rc=$(bash -c 'timeout 30 "$0" >/dev/null 2>&1; echo $?' "$D/m" 2>/dev/null)
if [ $rc -eq 0 ]; then echo "FAIL pin removed but test still passes (test proves nothing)"; exit 1; fi
echo "ok   without the pin the closure is lost (exit $rc)"
