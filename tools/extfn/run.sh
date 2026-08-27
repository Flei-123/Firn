#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/extfn/run.sh -- 'extern fn' MEASURED IN BOTH DIRECTIONS, IN BOTH
# COMPILERS (round 75, SPEC 14.1 item 7).
#
#   direction 1   Firn calls OUT: 'extern fn strlen(p: u64) -> u64;' is
#                 linked against a hand-written strlen (tools/extfn/impl.s,
#                 deliberately not libc) and against '#[link_name(strlen)]'
#                 under a DIFFERENT Firn-side name.
#   direction 2   Firn is called INTO: a '#[export_c]' function keeps its
#                 bare name, so an ordinary C program that never saw the
#                 Firn source can call it back.
#
# Both directions, both compilers (firnc0 in Rust, the self-hosted
# lib/firnc1/*.fi), six checks total, the exit code compared against the
# expectation baked into each source file.
#
# Usage:  bash tools/extfn/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FIRNC1=${FIRNC1:-./.firnc1}
WORK=.extfn-work
mkdir -p "$WORK"
rm -f "$WORK"/*

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
for t in as ld gcc; do
    command -v "$t" > /dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done

pass=0
fail=0

check() { # $1 = label, $2 = actual exit code, $3 = expected exit code
    if [ "$2" -eq "$3" ]; then
        echo "PASS  $1  (exit $2)"
        pass=$((pass + 1))
    else
        echo "FAIL  $1  (exit $2, expected $3)"
        fail=$((fail + 1))
    fi
}

# The hand-written strlen, assembled once, shared by direction 1's checks.
as -o "$WORK/impl.o" tools/extfn/impl.s || { echo "assembling impl.s failed"; exit 1; }

run_direction1() { # $1 = compiler binary, $2 = label prefix, $3 = source, $4 = expected
    local firnc=$1 label=$2 src=$3 expected=$4
    local obj="$WORK/${label}.o"
    "$firnc" "tools/extfn/$src" -o "$WORK/${label}" > /dev/null 2>"$WORK/${label}.build.log"
    # Expected to fail at `ld` (no libc linked) -- the object file is what
    # this test wants; the failure log is proof the symbol truly stayed
    # unresolved until we supply it ourselves.
    if [ ! -f "$obj" ]; then
        echo "FAIL  $label  (no object file produced, see $WORK/${label}.build.log)"
        fail=$((fail + 1))
        return
    fi
    ld -n -o "$WORK/${label}.bin" "$obj" "$WORK/impl.o" 2>"$WORK/${label}.link.log"
    if [ ! -x "$WORK/${label}.bin" ]; then
        echo "FAIL  $label  (link against impl.o failed, see $WORK/${label}.link.log)"
        fail=$((fail + 1))
        return
    fi
    "$WORK/${label}.bin"
    check "$label" "$?" "$expected"
}

run_direction2() { # $1 = compiler binary, $2 = label prefix, $3 = expected
    local firnc=$1 label=$2 expected=$3
    local obj="$WORK/${label}.o"
    # '-c'/'--object': ELF object file, no 'ld' -- 'main' stays UNRESOLVED
    # ('_start' calls it), which is fine here: host.c brings its own 'main'
    # and run.sh localizes Firn's '_start' so only host.c's entry point
    # runs. Both spellings of the Firn-side 'main' symbol are covered --
    # firnc0 emits it bare, lib/firnc1 emits it as '_F1.main'.
    "$firnc" -c tools/extfn/callback.fi -o "$obj" > /dev/null 2>"$WORK/${label}.build.log"
    if [ ! -f "$obj" ]; then
        echo "FAIL  $label  (no object file produced, see $WORK/${label}.build.log)"
        fail=$((fail + 1))
        return
    fi
    cp "$obj" "$WORK/${label}.clean.o"
    objcopy --localize-symbol=_start "$WORK/${label}.clean.o" 2>/dev/null
    objcopy --localize-symbol=main "$WORK/${label}.clean.o" 2>/dev/null
    objcopy --localize-symbol=_F1.main "$WORK/${label}.clean.o" 2>/dev/null
    gcc -o "$WORK/${label}.bin" tools/extfn/host.c "$WORK/${label}.clean.o" 2>"$WORK/${label}.link.log"
    if [ ! -x "$WORK/${label}.bin" ]; then
        echo "FAIL  $label  (gcc link failed, see $WORK/${label}.link.log)"
        fail=$((fail + 1))
        return
    fi
    "$WORK/${label}.bin"
    check "$label" "$?" "$expected"
}

# --- firnc0 (Rust) ----------------------------------------------------------
run_direction1 "$FIRNC" "stage0_callout"  callout.fi  5   # strlen("hello") = 5
run_direction1 "$FIRNC" "stage0_linkname" linkname.fi 3   # strlen("hi!")   = 3
run_direction2 "$FIRNC" "stage0_callback" 42               # add_one(41)    = 42

# --- lib/firnc1 (Firn, self-hosted) -----------------------------------------
if [ -x "$FIRNC1" ]; then
    run_direction1 "$FIRNC1" "stage1_callout"  callout.fi  5
    run_direction1 "$FIRNC1" "stage1_linkname" linkname.fi 3
    run_direction2 "$FIRNC1" "stage1_callback" 42
else
    echo "SKIP  stage 1 (no $FIRNC1 -- run tools/fixpoint.sh first, or build it directly)"
fi

echo "--------------------------------------------------------------------"
echo "PASS: $pass   FAIL: $fail"
[ "$fail" -eq 0 ]
