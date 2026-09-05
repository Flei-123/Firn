#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/abi/run.sh -- THE CALLING CONVENTION, MEASURED AGAINST GCC (round 71).
#
# WHY THIS EXISTS: up to round 70 an `f64` travelled in an INTEGER register.
# Within Firn that was consistent and correct, and SPEC 14.1.f64 named it as
# deviation F2. `f32` made the debt due: whoever wants to read a WAV or a
# glTF file talks to code that somebody else translated, and that code
# follows System V AMD64.
#
# "We follow System V now" is a claim. This is the measurement:
#
#   direction 1   GCC calls Firn functions. Wrong register -> wrong number.
#   direction 2   Firn calls GCC functions. The stubs in `probe.fi` are
#                 WEAKENED (`objcopy --weaken-symbol`), so the strong
#                 definitions in `host.c` win at link time.
#
# Both directions in one binary, 30 checks. The Firn object is linked into an
# ordinary C program; `_start` and `main` of Firn become LOCAL for that, so
# that the C entry point stays the one that runs.
#
# Usage:  bash tools/abi/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
WORK=.abi-work
mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
for t in gcc objcopy; do
    command -v "$t" > /dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done

build_probe() { # $1 = target object, $2... = additional firnc flags
    local out=$1; shift
    "$FIRNC" "$@" -c tools/abi/probe.fi -o "$out" || return 1
    # `_start`/`main` of Firn become local -- the C runtime brings its own.
    objcopy --localize-symbol=_start --localize-symbol=main "$out" || return 1
    # Every `cimpl_*` stub becomes weak; `host.c` defines the same symbol
    # strongly, and a strong definition beats a weak one.
    local stubs args s
    stubs=$(nm "$out" | awk '$2 == "T" && $3 ~ /^_F0\.cimpl_/ { print $3 }')
    if [ -z "$stubs" ]; then
        echo "ERROR: no cimpl stubs found -- has the symbol scheme changed?"
        return 1
    fi
    args=""
    for s in $stubs; do args="$args --weaken-symbol=$s"; done
    # shellcheck disable=SC2086
    objcopy $args "$out" || return 1
}

echo "== 1. translate the GCC side =="
gcc -O2 -Wall -c tools/abi/host.c -o "$WORK/host.o" || exit 1

echo "== 2. translate the Firn side, with and without the optimizer =="
build_probe "$WORK/probe_opt.o" || exit 1
build_probe "$WORK/probe_noopt.o" --no-opt || exit 1
gcc -O2 -o "$WORK/abi_opt" "$WORK/host.o" "$WORK/probe_opt.o" || exit 1
gcc -O2 -o "$WORK/abi_noopt" "$WORK/host.o" "$WORK/probe_noopt.o" || exit 1

FAIL=0
echo "== 3. GCC calls Firn (optimizer on) =="
"$WORK/abi_opt" || FAIL=1
echo "== 4. GCC calls Firn (optimizer off) =="
"$WORK/abi_noopt" || FAIL=1
echo "== 5. and back: Firn calls GCC =="
"$WORK/abi_noopt" caller || FAIL=1

if [ "$FAIL" -ne 0 ]; then
    echo "RESULT: the calling convention does NOT match GCC"
    exit 1
fi
echo "RESULT: 0 differing -- Firn and GCC hand over the same way"
exit 0
