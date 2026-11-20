#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
fail=0
check() {
    if [ "$2" = "$3" ]; then
        echo "ok   $1"
    else
        echo "FAIL $1: want '$2' got '$3'"
        fail=$((fail + 1))
    fi
}
run_a64() {
    "$FIRNC" --opt-level="$2" --target=aarch64-linux -o "$WORK/a64" "$1" >"$WORK/log" 2>&1 || { echo build; return; }
    qemu-aarch64 "$WORK/a64" >/dev/null 2>&1
    echo $?
}
run_x86() {
    "$FIRNC" --opt-level="$2" -o "$WORK/x86" "$1" >"$WORK/log" 2>&1 || { echo build; return; }
    "$WORK/x86" >/dev/null 2>&1
    echo $?
}
for lvl in dev-fast release-safe release-fast; do
    check "x86 operands $lvl" 0 "$(run_x86 "$ROOT/tests/6793_asm_operands_x86.fi" $lvl)"
    check "a64 operands $lvl" 0 "$(run_a64 "$ROOT/tests/asm_a64/operands.fi" $lvl)"
done
"$FIRNC" --target=aarch64-linux -o "$WORK/a64" "$ROOT/tests/asm_a64/locked_register.fi" >"$WORK/log" 2>&1
check "a64 locked register refused" 1 "$(grep -c "register 'x19' is not allowed" "$WORK/log")"
"$FIRNC" --unsafe=error --target=aarch64-linux -o "$WORK/a64" "$ROOT/tests/asm_a64/outside_unsafe.fi" >"$WORK/log" 2>&1
check "a64 asm outside unsafe refused" 1 "$(grep -c "inline assembly is only allowed" "$WORK/log")"
"$FIRNC" --unsafe=error --unsafe-scope=tests/6793 -o "$WORK/x86" "$ROOT/tests/6793_asm_operands_x86.fi" >"$WORK/log" 2>&1
check "x86 operands clean under --unsafe=error" 0 "$(grep -c '^error' "$WORK/log")"
[ "$fail" -eq 0 ] || { echo "$fail failed"; exit 1; }
echo "all ok"
