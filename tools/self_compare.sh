#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/self_compare.sh -- the compiler in FIRN compiles, the result
# RUNS, and it does the same as the one from `firnc0`.
#
# WHY NOT THE ASSEMBLY TEXT: `firnc0` has a register allocation
# (regalloc.rs), `lib/firnc1/codegen.fi` does not -- every value lies in the
# frame there. The two texts cannot be equal at all, and they do NOT
# have to be. What counts is the behaviour: the same return value, the same
# output.
#
# Sequence per file:
#   1. `.firnc1 file.fi -o target`  -- and ALL of it in Firn at that: lexing,
#      parsing, checking, lowering, producing code, calling `as` and `ld` over
#      `fork`/`execve`. The script calls NO tool itself.
#   2. run it, compare the return value and the standard output
#
# Return values of `.firnc1`: 3 = not core language * 4 = comptime *
# 5 = `defer` * 6 = the code generator cannot do this FIR (floating point,
# more than six arguments).
set -uo pipefail
cd "$(dirname "$0")/.."

# Module search path (round 39): the std facade for both sides of the comparison.
export FIRNLIB="$(pwd)/lib"

FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
WORK=.self-work
mkdir -p "$WORK"

# LESSON (round 46, the same trap for the fourth time): NEVER reuse a
# binary just because it exists. After a merge `.firnc1` is
# otherwise older than firnc0 or than the sources and the comparison measures
# a compiler that no longer exists. Round 45 reported such a
# seeming UNGLEICH in tests/771_gc_build_without_stw.fi that did not exist
# with a freshly built `.firnc1`.
rebuild=0
[ -x "$FC1" ] || rebuild=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && rebuild=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1
fi

same=0
different=0
noncore=0
comptime=0
defer_count=0
codegen=0
skipped=0
faulty=0
first=""

while IFS= read -r f; do
    # Only files that `firnc0` can compile -- otherwise they would be two
    # different inputs. Since round 29 files with `import` count
    # as well: `firnc1` resolves them itself.
    if ! "$FIRNC" "$f" -o "$WORK/ref" 2>/dev/null; then
        skipped=$((skipped+1))
        continue
    fi
    rm -f "$WORK/a.bin" "$WORK/a.bin.s" "$WORK/a.bin.o"
    "$FC1" "$f" -o "$WORK/a.bin" >/dev/null 2>&1
    rc=$?
    case "$rc" in
        3) noncore=$((noncore+1)); continue;;
        4) comptime=$((comptime+1)); continue;;
        5) defer_count=$((defer_count+1)); continue;;
        6) codegen=$((codegen+1)); continue;;
    esac
    if [ "$rc" -ne 0 ]; then
        faulty=$((faulty+1))
        [ -z "$first" ] && first="$f (firnc1 rc=$rc)"
        continue
    fi
    if [ ! -x "$WORK/a.bin" ]; then
        faulty=$((faulty+1))
        [ -z "$first" ] && first="$f (no executable file)"
        continue
    fi
    timeout 20 "$WORK/ref" > "$WORK/ref.out" 2>/dev/null
    rref=$?
    timeout 20 "$WORK/a.bin" > "$WORK/a.out" 2>/dev/null
    ra=$?
    if [ "$rref" -eq "$ra" ] && cmp -s "$WORK/ref.out" "$WORK/a.out"; then
        same=$((same+1))
    else
        different=$((different+1))
        [ -z "$first" ] && first="$f (firnc0: $rref, firnc1: $ra)"
    fi
done < <(find tests bench -name '*.fi' -not -type l -not -path 'tests/neg/*' -not -path 'tests/lexneg/*' | sort)

echo "SAME BEHAVIOUR:     $same"
echo "DIFFERING:          $different"
echo "FAULTY:             $faulty"
echo "NOT CORE:           $noncore"
echo "DEFER:              $defer_count"
echo "COMPTIME:           $comptime"
echo "CODEGEN MISSING:    $codegen  (floating point, more than six arguments)"
echo "SKIPPED:            $skipped  (firnc0 does not compile the file on its own)"
if [ -n "$first" ]; then
    echo "first deviation: $first"
    exit 1
fi
exit 0
