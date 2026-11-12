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
    if [ "$want" = "101" ]; then
        if [ "$rc" = 101 ] && printf '%s' "$out" | grep -q "firn: stack overflow"; then
            echo "ok   $target $prog $level exit 101 + message"
        else
            echo "FAIL $target $prog $level: exit $rc, wanted 101 + message"
            fail=1
        fi
    elif [ "$rc" = 101 ]; then
        echo "FAIL $target $prog $level: exit 101, wanted unguarded"
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
        main_want=101
        [ "$level" = release-fast ] && main_want=none
        check "$target" ub05_stack_overflow "$level" "$main_want" "$runner" "$ext"
        check "$target" ub26_thread_stack_overflow "$level" 101 "$runner" "$ext"
    done
done
exit $fail
