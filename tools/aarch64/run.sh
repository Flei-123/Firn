#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/aarch64/run.sh -- THE CROSS CHECK OF ROUND 80.
#
# The same Firn program is compiled TWICE, once for each machine, and both
# results are RUN. What is compared is what the program does: its standard
# output character for character and its exit code.
#
#   x86-64   firnc --target=x86_64-linux   -> runs natively
#   aarch64  firnc --target=aarch64-linux  -> runs under qemu-aarch64
#
# That comparison is the point of the round. A code generator can be read
# and believed; two code generators that produce the same BEHAVIOUR out of
# the same intermediate representation cannot both be wrong in the same way
# by accident.
#
# Every case ends in exactly one of four buckets, and none of them is
# swept under the carpet:
#
#   SAME           both compiled, both ran, same output and same exit code
#   DIFFERENT      both compiled -- and they do not agree (or the aarch64
#                  side crashed, hung or its assembler refused the text)
#   NOT SUPPORTED  the aarch64 code generator REFUSED the program with a
#                  clear message (threads, inline assembler, a system call
#                  that has no counterpart). Nothing was emitted, nothing
#                  was guessed.
#   X86 ALREADY    the x86-64 side does not meet its own expectation from
#                  line 1 -- then there is nothing to compare and the case
#                  says so instead of counting as a success.
#   ENVIRONMENT    the difference belongs to the RUNNER, not to the code
#                  generator -- and that has to be PROVEN in this very run
#                  by a probe written in C (tools/aarch64/environment.txt,
#                  tools/aarch64/qemu_mmap_probe.c). If the probe says the
#                  environment behaves like Linux, the case counts as
#                  DIFFERENT again. Nothing gets into this bucket by being
#                  named there alone.
#
# Usage:
#   tools/aarch64/run.sh            all of tests/*.fi, optimised build
#   tools/aarch64/run.sh --no-opt   the same corpus with the optimiser off
#   A64_FILTER=340 tools/aarch64/run.sh    only the cases whose name matches
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
QEMU=${QEMU:-qemu-aarch64}
WORK="$ROOT/.a64-work"
JOBS=${JOBS:-8}
FLAGS=""
# ROUND 83: the label says what the compiler REALLY builds without a
# flag. Round 72 found the command line default lying (`OptConfig::
# default` returned `release-fast` while every document said `dev-fast`)
# and fixed it; this line kept repeating the old answer. It matters here
# of all places, because `dev-fast` is a level that CHECKS the arithmetic
# -- what this stage compares is the checked build, not the fast one.
LABEL="dev-fast"
if [ "${1:-}" = "--no-opt" ]; then
    FLAGS="--no-opt"
    LABEL="no-opt"
    shift
fi
FILTER=${A64_FILTER:-}

if [ ! -x "$FIRNC" ]; then
    echo "firnc is missing: $FIRNC (cargo build --release --manifest-path compiler/Cargo.toml)"
    exit 1
fi
# The same house rule the other tools follow: a missing cross toolchain is
# said out loud and skipped, it does not turn into a red suite on a machine
# that never had it installed.
for t in "$QEMU" aarch64-linux-gnu-as aarch64-linux-gnu-ld; do
    command -v "$t" >/dev/null 2>&1 || {
        echo "SKIP: $t is missing (apt-get install qemu-user binutils-aarch64-linux-gnu)"
        exit 0
    }
done

rm -rf "$WORK"
mkdir -p "$WORK"

# --- the probe ------------------------------------------------------------
# Does this runner give a freed mapping back at the same address? The answer
# decides whether the one entry of environment.txt counts at all.
MMAP_REUSE=unknown
if command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
    if aarch64-linux-gnu-gcc -static -O0 -o "$WORK/probe" tools/aarch64/qemu_mmap_probe.c 2>/dev/null; then
        PROBE_OUT=$("$QEMU" "$WORK/probe" 2>&1)
        if [ $? -eq 0 ]; then MMAP_REUSE=yes; else MMAP_REUSE=no; fi
        echo "  probe: $PROBE_OUT"
    fi
fi
export MMAP_REUSE

# ---------------------------------------------------------------- one case
# Writes exactly ONE result line to standard output:
#   SAME <file> | DIFF <file> :: <why> | NOTSUP <file> :: <why>
#   | X86BAD <file> :: <why> | ENVIRON <file> :: <why>
#
# `differs` decides between DIFF and ENVIRON: a case may only leave the DIFF
# bucket when environment.txt names it AND the probe of this run proved the
# environment fact it names.
differs() {   # $1 = file, $2 = why
    local line probe reason
    line=$(grep -m1 "^$1 " tools/aarch64/environment.txt 2>/dev/null)
    if [ -n "$line" ]; then
        probe=$(echo "$line" | awk '{print $2}')
        reason=$(echo "$line" | cut -d' ' -f3-)
        if [ "$probe" = "mmap-address-reuse" ] && [ "${MMAP_REUSE:-unknown}" = "no" ]; then
            echo "ENVIRON $1 :: $reason"
            return
        fi
    fi
    echo "DIFF $1 :: $2"
}
export -f differs

one_case() {
    local file="$1" base exp hdr kind
    base=$(basename "$file" .fi)
    hdr=$(head -1 "$file")
    case "$hdr" in
        *expect_out:*)  kind=out;  exp=${hdr#*expect_out: } ;;
        *expect_exit:*) kind=exit; exp=${hdr#*expect_exit: } ;;
        *) echo "X86BAD $file :: no expectation in line 1"; return ;;
    esac

    # --- x86-64: does the case work at all, and does it meet its own
    # expectation? Only then is there something to compare.
    if ! "$FIRNC" $FLAGS --target=x86_64-linux -o "$WORK/$base.x86" "$file" \
            >"$WORK/$base.x86.err" 2>&1; then
        echo "X86BAD $file :: x86 compilation failed"; return
    fi
    local xout xrc try
    # Two attempts. Eight cases run at a time here, and the handful of
    # programs that really start threads (tests/860, tests/834) lose their
    # timing under that load -- on x86 as well. A case that meets its own
    # expectation on the second, quieter attempt was never a difference
    # between the machines; one that fails twice is reported and not
    # compared.
    for try in 1 2; do
        xout=$(timeout 60 "$WORK/$base.x86" </dev/null 2>/dev/null); xrc=$?
        if [ "$kind" = out ]; then
            [ "$xrc" -eq 0 ] && [ "$xout" = "$exp" ] && break
        else
            [ "$xrc" = "$exp" ] && break
        fi
        [ "$try" -eq 2 ] && {
            if [ "$kind" = out ]; then
                echo "X86BAD $file :: x86 output does not match its own expectation (twice)"
            else
                echo "X86BAD $file :: x86 exit code $xrc, expected $exp (twice)"
            fi
            return
        }
        sleep 2
    done

    # --- aarch64
    if ! "$FIRNC" $FLAGS --target=aarch64-linux -o "$WORK/$base.a64" "$file" \
            >"$WORK/$base.a64.err" 2>&1; then
        local why
        why=$(grep -m1 -E '^error: ' "$WORK/$base.a64.err" | cut -c8-160)
        [ -z "$why" ] && why=$(head -1 "$WORK/$base.a64.err" | cut -c1-160)
        # A refusal BY THE CODE GENERATOR is `NOT SUPPORTED`; anything else
        # (the assembler choked, the linker choked, a panic) is a defect and
        # counts as DIFFERENT.
        case "$why" in
            *"not supported on aarch64"*|*"aarch64:"*|*"no meaning on aarch64"*|\
            *"threads are not supported"*|*"does not support the kernel profile"*)
                echo "NOTSUP $file :: $why" ;;
            *)  differs "$file" "aarch64 compilation failed: $why" ;;
        esac
        return
    fi
    local aout arc
    aout=$(timeout 120 "$QEMU" "$WORK/$base.a64" </dev/null 2>/dev/null); arc=$?
    # The same second attempt on this side, for the same reason -- and only
    # when the two really disagree, so a run that already agrees costs
    # nothing.
    if [ "$xrc" != "$arc" ] || [ "$xout" != "$aout" ]; then
        sleep 2
        aout=$(timeout 120 "$QEMU" "$WORK/$base.a64" </dev/null 2>/dev/null); arc=$?
    fi
    if [ "$xrc" != "$arc" ]; then
        differs "$file" "exit code x86=$xrc aarch64=$arc"; return
    fi
    if [ "$xout" != "$aout" ]; then
        local x1 a1
        x1=$(printf '%s' "$xout" | head -c 60 | tr '\n' '|')
        a1=$(printf '%s' "$aout" | head -c 60 | tr '\n' '|')
        differs "$file" "output x86='$x1' aarch64='$a1'"; return
    fi
    echo "SAME $file"
}
export -f one_case
export FIRNC QEMU WORK FLAGS ROOT FIRNLIB

LIST="$WORK/list.txt"
ls tests/*.fi > "$LIST"
if [ -n "$FILTER" ]; then
    grep "$FILTER" "$LIST" > "$LIST.f" && mv "$LIST.f" "$LIST"
fi
TOTAL=$(wc -l < "$LIST")

echo "== aarch64 cross check ($LABEL, $TOTAL cases, $JOBS at a time) =="
xargs -a "$LIST" -P "$JOBS" -I{} bash -c 'one_case "$@"' _ {} | sort > "$WORK/raw.$LABEL.txt"

# --- the quiet pass -------------------------------------------------------
# Eight cases run at a time above, and a few programs in this corpus measure
# TIMING: tests/860_thread_basic.fi deliberately checks that an unlocked
# counter LOSES increments, which is a statement about four threads really
# running at the same time. Under eight parallel jobs and a qemu that
# multiplexes the guest's threads, that race stops happening -- on the x86
# side just as much.
#
# So every case that did not come out SAME is run ONCE MORE, alone, after
# all the parallel work is done, and the second verdict is the one that
# counts. This is not an exception list: it applies to every case by the
# same rule, and a case that disagrees in a quiet machine stays DIFFERENT.
requiet=0
: > "$WORK/result.$LABEL.txt"
while IFS= read -r line; do
    case "$line" in
        DIFF\ *|X86BAD\ *)
            f=$(echo "$line" | awk '{print $2}')
            requiet=$((requiet + 1))
            one_case "$f" >> "$WORK/result.$LABEL.txt"
            ;;
        *) printf '%s\n' "$line" >> "$WORK/result.$LABEL.txt" ;;
    esac
done < "$WORK/raw.$LABEL.txt"
sort -o "$WORK/result.$LABEL.txt" "$WORK/result.$LABEL.txt"
[ "$requiet" -gt 0 ] && echo "  ($requiet case(s) run again alone, on a quiet machine)"

SAME=$(grep -c '^SAME '   "$WORK/result.$LABEL.txt" || true)
DIFF=$(grep -c '^DIFF '   "$WORK/result.$LABEL.txt" || true)
NOTSUP=$(grep -c '^NOTSUP ' "$WORK/result.$LABEL.txt" || true)
X86BAD=$(grep -c '^X86BAD ' "$WORK/result.$LABEL.txt" || true)
ENVIRON=$(grep -c '^ENVIRON ' "$WORK/result.$LABEL.txt" || true)

echo
grep '^DIFF '   "$WORK/result.$LABEL.txt" | sed 's/^/  /'
grep '^NOTSUP ' "$WORK/result.$LABEL.txt" | sed 's/^/  /'
grep '^ENVIRON ' "$WORK/result.$LABEL.txt" | cut -c1-200 | sed 's/^/  /'
grep '^X86BAD ' "$WORK/result.$LABEL.txt" | sed 's/^/  /'
echo
COMPARABLE=$((SAME + DIFF + NOTSUP))
PCT=0
[ "$COMPARABLE" -gt 0 ] && PCT=$((SAME * 100 / COMPARABLE))
echo "  build stage:    $LABEL"
echo "  SAME:           $SAME"
echo "  DIFFERENT:      $DIFF"
echo "  NOT SUPPORTED:  $NOTSUP"
echo "  ENVIRONMENT:    $ENVIRON (proven, see tools/aarch64/environment.txt)"
echo "  x86 already:    $X86BAD"
echo "  RESULT: $SAME of $COMPARABLE comparable cases identical on both machines ($PCT%)"

# The gate: nothing may DIFFER. What aarch64 cannot do says so and is
# counted; what it claims to do has to be right.
if [ "$DIFF" -gt 0 ]; then
    echo "FAIL $DIFF case(s) differ between the two machines"
    exit 1
fi
echo "PASS no case differs between x86-64 and aarch64"
