#!/usr/bin/env bash
# r902: a Windows PE built by firnc has .rodata read-only (no write flag), no W+X section.
# SKIP when the mingw binutils are missing.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
OBJDUMP=x86_64-w64-mingw32-objdump
if ! command -v "$OBJDUMP" >/dev/null 2>&1 || ! command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    echo "SKIP pe rodata: mingw binutils missing"
    exit 0
fi
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
bad=0
for f in examples/fib.fi examples/hello.fi examples/tour.fi; do
    b=$(basename "$f" .fi)
    if ! "$FIRNC" --target=x86_64-windows -o "$W/$b.exe" "$ROOT/$f" >/dev/null 2>&1; then
        echo "FAIL $b: compile"
        bad=1
        continue
    fi
    ro=$("$OBJDUMP" -h "$W/$b.exe" | grep -A1 ' \.rodata ' | tr '\n' ' ')
    case "$ro" in
        *READONLY*) ;;
        *) echo "FAIL $b: .rodata not READONLY: $ro"; bad=1 ;;
    esac
    if "$OBJDUMP" -p "$W/$b.exe" | grep -q 'Characteristics 0xe0'; then
        echo "FAIL $b: W+X section"
        bad=1
    fi
done
[ "$bad" -eq 0 ] && echo "pe rodata: read-only in 3 images"
exit "$bad"
