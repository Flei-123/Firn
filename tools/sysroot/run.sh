#!/usr/bin/env bash
# tools/sysroot/run.sh -- firnc finds the standard library without $FIRNLIB.
#
# Layouts checked (firnc0 always; firnc1 when $FIRNC1 names a built binary):
#   1. repository: compiler/target/release/firnc -> ../../../lib
#   2. installed:  <prefix>/bin/firnc + <prefix>/lib
#   3. deeper:     <prefix>/a/b/firnc + <prefix>/lib   (../../lib)
#   4. $FIRNLIB wins over the compiler-relative lib
#   5. `--print-sysroot` (firnc0): path, $FIRNLIB wins, exit 1 without a lib
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
FC1=${FIRNC1:-}
[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
fail=0
say() { printf '  %-58s %s\n' "$1" "$2"; }
check() { # name wanted got
    if [ "$2" = "$3" ]; then say "$1" OK; else
        say "$1" DIFFERENT; echo "      wanted: $2"; echo "      got   : $3"; fail=$((fail + 1)); fi
}
cat > "$D/hello.fi" <<'SRC'
import std.io

fn main() -> i32 {
    io.print_line("sysroot ok")
    return 0
}
SRC
build_run() { # compiler  -> prints program output or the compiler error
    local c=$1 out
    rm -f "$D/hello"
    out=$(cd "$D" && env -u FIRNLIB "$c" hello.fi -o hello 2>&1) || { echo "BUILD FAILED: $out" | head -2 | tr '\n' ' '; return; }
    "$D/hello"
}
# installed layouts: lib is a symlink to the repo lib, the compiler a copy
mkdir -p "$D/inst/bin" "$D/deep/a/b" "$D/nolib/bin"
ln -s "$ROOT/lib" "$D/inst/lib"
ln -s "$ROOT/lib" "$D/deep/lib"
cp "$FIRNC" "$D/inst/bin/firnc"
cp "$FIRNC" "$D/deep/a/b/firnc"
cp "$FIRNC" "$D/nolib/bin/firnc"
compilers=("repo:$FIRNC" "inst:$D/inst/bin/firnc" "deep:$D/deep/a/b/firnc")
if [ -n "$FC1" ] && [ -x "$FC1" ]; then
    mkdir -p "$D/inst1/bin"; ln -s "$ROOT/lib" "$D/inst1/lib"; cp "$FC1" "$D/inst1/bin/firnc1"
    mkdir -p "$D/repo1/compiler/target/release"; ln -s "$ROOT/lib" "$D/repo1/lib"
    cp "$FC1" "$D/repo1/compiler/target/release/firnc1"
    compilers+=("inst1:$D/inst1/bin/firnc1" "repo1:$D/repo1/compiler/target/release/firnc1")
fi
for e in "${compilers[@]}"; do
    check "no FIRNLIB, layout ${e%%:*}" "sysroot ok" "$(build_run "${e#*:}")"
done
# an unusable $FIRNLIB dir does not hide the sysroot (the search continues)
mkdir -p "$D/empty"
bad=$(cd "$D" && FIRNLIB=$D/empty "$FIRNC" hello.fi -o hello 2>&1 >/dev/null | head -1 | cut -c1-5)
check "FIRNLIB set but without the module: sysroot still used" "" "$bad"
# --print-sysroot
check "--print-sysroot (installed)" "$D/inst/bin/../lib" "$(env -u FIRNLIB "$D/inst/bin/firnc" --print-sysroot)"
check "--print-sysroot (deep)" "$D/deep/a/b/../../lib" "$(env -u FIRNLIB "$D/deep/a/b/firnc" --print-sysroot)"
check "--print-sysroot: FIRNLIB wins" "/x/lib" "$(FIRNLIB=/x/lib "$D/inst/bin/firnc" --print-sysroot)"
env -u FIRNLIB "$D/nolib/bin/firnc" --print-sysroot > /dev/null 2>&1
check "--print-sysroot without a lib: exit 1" "1" "$?"
if [ "$fail" -ne 0 ]; then echo "sysroot: $fail FAILED"; exit 1; fi
echo "sysroot: all OK"
