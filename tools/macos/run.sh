#!/bin/bash
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
export FIRNLIB="${FIRNLIB:-$ROOT/lib}"
export PATH="/usr/lib/llvm-14/bin:$PATH"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PROGRAMS="tests/001_return_const.fi tests/1331_str_concat.fi tests/1610_hash_map.fi tests/2101_threads_portable.fi examples/hello.fi examples/tour.fi"
TARGETS="aarch64-macos x86_64-macos"
PASS=0
FAIL=0

check() {
    local label="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "FAIL: $label"
    fi
}

has() { grep -q -- "$2" "$1"; }

for target in $TARGETS; do
    arch=arm64
    [ "$target" = x86_64-macos ] && arch=x86_64
    for prog in $PROGRAMS; do
        name="$(basename "$prog" .fi)-$target"
        out="$WORK/$name"
        if ! "$FIRNC" --target="$target" "$ROOT/$prog" -o "$out" 2>"$WORK/$name.err"; then
            FAIL=$((FAIL + 1))
            echo "FAIL: $name compile: $(head -1 "$WORK/$name.err")"
            continue
        fi
        llvm-objdump --macho --private-headers "$out" >"$WORK/$name.hdr" 2>&1
        llvm-nm -u "$out" >"$WORK/$name.undef" 2>&1
        check "$name filetype" has "$WORK/$name.hdr" "EXECUTE"
        check "$name arch" has "$WORK/$name.hdr" "$arch\|ARM64\|X86_64"
        check "$name LC_MAIN" has "$WORK/$name.hdr" "LC_MAIN"
        check "$name libSystem" has "$WORK/$name.hdr" "libSystem.B.dylib"
        check "$name platform" has "$WORK/$name.hdr" "platform macos"
        check "$name no RWX segment" bash -c "! grep -E 'maxprot rwx' '$WORK/$name.hdr' | grep -v __PAGEZERO | grep -v LINKEDIT | grep -q ."
        unknown="$(grep -v -E '^\s*(_write|_read|_open|_close|_lseek|__exit|_mmap|_munmap|_mprotect|_clock_gettime|_nanosleep|_getpid|_sched_yield|_fsync|_ftruncate|_unlink|_mkdir|_pipe|_dup2|_kill|___error|dyld_stub_binder)\s*$' "$WORK/$name.undef" | grep -v -E ':$|^$' | head -3)"
        check "$name only libSystem imports" test -z "$unknown"
    done
done
echo "macos structural checks: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
