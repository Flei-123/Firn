#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
export WINEDEBUG=-all
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
STACK_MESSAGE="firn: stack overflow"
NULL_MESSAGE="firn: null pointer dereference"
check() {
    local target=$1 prog=$2 level=$3 want=$4 runner=$5 ext=$6
    local exe="$W/$prog.$level.$ext"
    if ! "$FIRNC" --target="$target" --opt-level="$level" -o "$exe" "$ROOT/tools/safety/ub/$prog.fi" >/dev/null 2>&1; then
        echo "FAIL $target $prog $level: compile error"
        fail=1
        return
    fi
    local out rc
    out=$(timeout 60 $runner "$exe" 2>&1 >/dev/null)
    rc=$?
    case $want in
        stack) message=$STACK_MESSAGE ;;
        null) message=$NULL_MESSAGE ;;
        *) message= ;;
    esac
    if [ -n "$message" ]; then
        if [ "$rc" = 101 ] && printf '%s' "$out" | grep -q "$message"; then
            echo "ok   $target $prog $level exit 101 + message"
        else
            echo "FAIL $target $prog $level: exit $rc, wanted 101 + '$message'"
            fail=1
        fi
    elif [ "$rc" = 101 ] || printf '%s' "$out" | grep -q "firn: "; then
        echo "FAIL $target $prog $level: exit $rc, wanted unguarded"
        fail=1
    else
        echo "ok   $target $prog $level unguarded (exit $rc)"
    fi
}
for target in aarch64-linux x86_64-windows; do
    case $target in
        aarch64-linux) runner=qemu-aarch64; ext=elf ;;
        *) runner=wine; ext=exe ;;
    esac
    for level in dev-fast release-safe release-fast; do
        check "$target" ub05_stack_overflow "$level" stack "$runner" "$ext"
        check "$target" ub26_thread_stack_overflow "$level" stack "$runner" "$ext"
        for prog in ub01_null_read ub02_null_write ub03_gc_null_field_write ub04_gc_null_field_read; do
            check "$target" "$prog" "$level" null "$runner" "$ext"
        done
        check "$target" ub28_wild_pointer "$level" none "$runner" "$ext"
    done
done
exit $fail
