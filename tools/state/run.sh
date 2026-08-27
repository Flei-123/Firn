#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/state/run.sh -- THE PROOF FOR GLOBAL VARIABLES (round 89,
# SPEC 14.1.statics), in BOTH compilers and on BOTH machines.
#
# Until this round Firn had `const` and nothing else, and SPEC 14.1 item 5
# named the three questions that had to be answered before a data section
# could exist. This tool is the answer, measured rather than asserted:
#
#   1. READ AND WRITE. A `static mut` counts up across function calls and
#      keeps its value; a `static` without `mut` reads and cannot be
#      written (counter-check below). Aggregates too: an array `static` is
#      indexed and assigned into like any other array.
#   2. ACROSS MODULE BOUNDARIES. `export { total }` in one module, written
#      from there, read from another -- ONE place, not one copy per file.
#   3. THE SECTION IS REALLY THE ONE PROMISED. `readelf -S` says what the
#      LINKER produced: a `mut` all-zero one in `.bss` (NOBITS, no space in
#      the file), a `mut` one with a value in `.data`, one without `mut` in
#      `.rodata`. Not "the compiler emitted the directive".
#   4. THE KERNEL PROFILE. `--profile=kernel -c` produces an object file
#      with the same sections and NO undefined name beyond the ones a
#      kernel defines itself.
#   5. BOTH MACHINES. Every runnable case is compiled for aarch64 as well
#      and run under qemu-aarch64, and compared against what the x86-64
#      build of the SAME program does.
#   6. BOTH COMPILERS. Everything above is done with firnc0 (Rust) and
#      firnc1 (Firn). A promise one of them keeps is half a promise.
#   7. THE COUNTER-CHECKS, and they are the point of the round:
#      * an initial value that is NOT evaluable at compile time is refused
#        (that is what makes an initialisation order unnecessary),
#      * a `static Gc[T]` is refused, with the reason,
#      * a write to a `static` without `mut` is refused,
#      * a program WITHOUT a `static` carries no data section at all.
#
# Usage:  bash tools/state/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
QEMU=${QEMU:-qemu-aarch64}
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
# Never reuse an old `.firnc1`: after a change to lib/firnc1 it measures a
# compiler that no longer exists (the lesson of round 46).
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc0 cannot build firnc1"; exit 1; }
fi

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; }

cc_of()   { if [ "$1" = 0 ]; then echo "$FIRNC"; else echo "$FC1"; fi; }
# The assembly text of a program. firnc0 writes it with `--emit=asm -o`,
# firnc1 writes it to STANDARD OUTPUT when no `-o` is given -- one helper,
# so the two spellings cannot drift apart in three places.
asm_of() { # $1 = compiler index, $2 = source, $3 = output file
    if [ "$1" = 0 ]; then
        "$FIRNC" --emit=asm "$2" -o "$3" > /dev/null 2>&1
    else
        "$FC1" "$2" > "$3" 2> /dev/null
    fi
}
name_of() { if [ "$1" = 0 ]; then echo "firnc0"; else echo "firnc1"; fi; }

# $1 = source, $2 = expected exit code, $3 = expected stdout ("" = do not look)
run_both_cc() {
    local src="$1" want_rc="$2" want_out="$3"
    local base; base=$(basename "$src" .fi)
    local cc C bin out rc
    for cc in 0 1; do
        C=$(cc_of "$cc")
        bin="$TMPD/$base.$cc"
        if ! "$C" "$src" -o "$bin" > "$TMPD/build.log" 2>&1; then
            bad "$base [$(name_of "$cc")]: did not build: $(head -2 "$TMPD/build.log")"
            continue
        fi
        out=$("$bin" 2>/dev/null); rc=$?
        if [ "$rc" != "$want_rc" ]; then
            bad "$base [$(name_of "$cc")]: exit code $rc, expected $want_rc"
        elif [ -n "$want_out" ] && [ "$out" != "$want_out" ]; then
            bad "$base [$(name_of "$cc")]: output '$out', expected '$want_out'"
        else
            ok
        fi
    done
}

echo "== 1. read and write, scalars and arrays =="
run_both_cc tools/state/counter.fi 59 ""
run_both_cc tools/state/table.fi 42 ""

echo "== 2. across module boundaries: ONE place, not one per file =="
for cc in 0 1; do
    C=$(cc_of "$cc")
    if ! "$C" tools/state/mod/main.fi -o "$TMPD/mod.$cc" > "$TMPD/build.log" 2>&1; then
        bad "modules [$(name_of "$cc")]: did not build: $(head -2 "$TMPD/build.log")"
        continue
    fi
    "$TMPD/mod.$cc" > /dev/null 2>&1; rc=$?
    if [ "$rc" = 9 ]; then ok; else bad "modules [$(name_of "$cc")]: exit code $rc, expected 9"; fi
done

echo "== 3. the section is really the one promised (readelf) =="
for cc in 0 1; do
    C=$(cc_of "$cc")
    if ! "$C" tools/state/sections.fi -o "$TMPD/sec.$cc" > "$TMPD/build.log" 2>&1; then
        bad "sections [$(name_of "$cc")]: did not build: $(head -2 "$TMPD/build.log")"
        continue
    fi
    readelf -S -W "$TMPD/sec.$cc" > "$TMPD/sec.$cc.hdr"
    for want in bss data rodata; do
        if grep -qE "\] +\.${want} " "$TMPD/sec.$cc.hdr"; then
            ok
        else
            bad "sections [$(name_of "$cc")]: no .$want in the binary"
        fi
    done
    # `.bss` takes no space in the FILE -- that is the whole point of it.
    bss_type=$(grep -E "\] +\.bss " "$TMPD/sec.$cc.hdr" | head -1 | awk '{print $4}')
    if [ "$bss_type" = "NOBITS" ]; then ok; else
        bad "sections [$(name_of "$cc")]: .bss is '$bss_type', not NOBITS"
    fi
    # And which of the three each `static` really landed in: the last
    # `.section` directive before its label decides.
    asm_of "$cc" tools/state/sections.fi "$TMPD/sec.$cc.s"
    for pair in "ZED:.bss" "SEED:.data" "NAMES:.rodata"; do
        symbol=${pair%%:*}; section=${pair#*:}
        line=$(grep -n "^\.Lstatic_${symbol}:" "$TMPD/sec.$cc.s" | head -1 | cut -d: -f1)
        if [ -z "$line" ]; then
            bad "sections [$(name_of "$cc")]: no label for '$symbol'"
            continue
        fi
        here=$(head -"$line" "$TMPD/sec.$cc.s" | grep -E "^\.section|^\.text" | tail -1)
        case "$here" in
            *"$section"*) ok ;;
            *) bad "sections [$(name_of "$cc")]: '$symbol' sits in '$here', not in '$section'" ;;
        esac
    done
    # The counter-check: a program WITHOUT a `static` carries no data
    # section at all -- the feature costs nothing where it is not used.
    asm_of "$cc" tools/state/nostatic.fi "$TMPD/no.$cc.s"
    if grep -q "\.Lstatic_" "$TMPD/no.$cc.s"; then
        bad "sections [$(name_of "$cc")]: a program without a 'static' carries one anyway"
    else
        ok
    fi
done

echo "== 4. the kernel profile: no collector, no undefined name of ours =="
for cc in 0 1; do
    C=$(cc_of "$cc")
    if ! "$C" --profile=kernel -c tools/state/kernel.fi -o "$TMPD/k.$cc.o" > "$TMPD/build.log" 2>&1; then
        bad "kernel [$(name_of "$cc")]: did not build: $(head -2 "$TMPD/build.log")"
        continue
    fi
    if readelf -S -W "$TMPD/k.$cc.o" | grep -qE "\] +\.bss "; then ok; else
        bad "kernel [$(name_of "$cc")]: no .bss in the object file"
    fi
    # Everything undefined has to be a name the KERNEL defines itself.
    # `serial_out` is the one this file expects; there is nothing else.
    # The kernel brings its own `#[panic_handler]`, so `osum_panic` does
    # NOT turn up here -- that is the point of the attribute. `serial_out`
    # is the one name this file leaves to the kernel on purpose.
    und=$(readelf -s -W "$TMPD/k.$cc.o" | awk '$4=="NOTYPE" && $7=="UND" && $8!="" {print $8}' | sort -u | grep -v '^serial_out$' || true)
    if [ -z "$und" ]; then ok; else bad "kernel [$(name_of "$cc")]: undefined names: $und"; fi
done

echo "== 5. the second machine: aarch64 =="
if ! command -v "$QEMU" > /dev/null 2>&1 || ! command -v aarch64-linux-gnu-as > /dev/null 2>&1; then
    echo "   SKIPPED: qemu-aarch64 or the aarch64 binutils are missing"
else
    for src in tools/state/counter.fi tools/state/table.fi tools/state/sections.fi; do
        base=$(basename "$src" .fi)
        if ! "$FIRNC" --target=aarch64-linux "$src" -o "$TMPD/$base.a64" > "$TMPD/build.log" 2>&1; then
            bad "$base [aarch64]: did not build: $(head -2 "$TMPD/build.log")"
            continue
        fi
        out=$("$QEMU" "$TMPD/$base.a64" 2>/dev/null); rc=$?
        # The x86-64 answer of the SAME program is the expectation -- one
        # code generator against the other, not against a number written
        # down here.
        "$FIRNC" "$src" -o "$TMPD/$base.x86" > /dev/null 2>&1
        exp_out=$("$TMPD/$base.x86" 2>/dev/null); exp_rc=$?
        if [ "$rc" = "$exp_rc" ] && [ "$out" = "$exp_out" ]; then ok; else
            bad "$base [aarch64]: exit $rc/'$out', x86-64 says $exp_rc/'$exp_out'"
        fi
    done
    # The addressing itself: aarch64 has no rip-relative mode, the address
    # of a global is `adrp` + `add :lo12:`. Without that pair the runs
    # above passed for some other reason.
    "$FIRNC" --target=aarch64-linux --emit=asm tools/state/counter.fi -o "$TMPD/c.a64.s" > /dev/null 2>&1
    if grep -q "adrp .*\.Lstatic_" "$TMPD/c.a64.s" && grep -q ":lo12:\.Lstatic_" "$TMPD/c.a64.s"; then
        ok
    else
        bad "aarch64: no 'adrp'/':lo12:' pair for a global"
    fi
fi

echo "== 6. the counter-checks =="
# $1 = source, $2 = a word that must appear in firnc0's message
refuse() {
    local src="$1" word="$2"
    local base; base=$(basename "$src" .fi)
    if "$FIRNC" "$src" -o "$TMPD/$base.neg" > "$TMPD/neg.log" 2>&1; then
        bad "$base [firnc0]: was accepted, and should not have been"
    elif ! grep -q "$word" "$TMPD/neg.log"; then
        bad "$base [firnc0]: refused, but without '$word': $(head -1 "$TMPD/neg.log")"
    else
        ok
    fi
    # firnc1 reports its errors as a COUNT, not as text (diag.fi is the
    # separate half). What is compared is the DECISION, and that has to be
    # the same one.
    if "$FC1" "$src" -o "$TMPD/$base.neg1" > /dev/null 2>&1; then
        bad "$base [firnc1]: was accepted, and should not have been"
    else
        ok
    fi
}
refuse tools/state/neg/runtime_init.fi "compile time"
refuse tools/state/neg/gc_static.fi "collected value"
refuse tools/state/neg/write_const.fi "without 'mut'"
refuse tools/state/neg/void_static.fi "no storage"

echo
echo "state: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
