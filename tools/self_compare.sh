#!/usr/bin/env bash
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

FIRNC=${FIRNC:-compiler/target/release/firnc}
FC1=${FIRNC1:-./.firnc1}

# ROUND r198: a work directory of its own per run (mktemp), removed on exit.
# It used to be the fixed `.self-work`, so two simultaneous runs in the same
# tree (fixpoint.sh + a shard, two worktree users) overwrote each other's
# ref/a.bin/*.out and reported deviations that did not exist.
# Override: SELF_COMPARE_WORK=<dir> (kept after the run, not deleted).
if [ -n "${SELF_COMPARE_WORK:-}" ]; then
    WORK=$SELF_COMPARE_WORK
    mkdir -p "$WORK"
else
    WORK=$(mktemp -d)
    trap 'rm -rf "$WORK"' EXIT
fi

# Shard mode: SELF_COMPARE_SHARD=k/N runs only every N-th corpus file
# (index k of 0..N-1). Run N shards in parallel (each with its own WORK) and
# add the counters up.
SHARD_K=0
SHARD_N=1
if [ -n "${SELF_COMPARE_SHARD:-}" ]; then
    SHARD_K=${SELF_COMPARE_SHARD%/*}
    SHARD_N=${SELF_COMPARE_SHARD#*/}
    case "$SHARD_K$SHARD_N" in *[!0-9]*|"") echo "bad SELF_COMPARE_SHARD (want k/N)"; exit 2;; esac
    if [ "$SHARD_N" -lt 1 ] || [ "$SHARD_K" -ge "$SHARD_N" ]; then
        echo "bad SELF_COMPARE_SHARD (want 0 <= k < N)"; exit 2
    fi
fi

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
    # Build next to the target and rename: the rename is atomic, so a parallel
    # run never sees (or executes) a half-written compiler.
    "$FIRNC" --opt-level="${FIRNC1_OPT:-release-fast}" bin/firnc1.fi -o "$WORK/firnc1.new" || exit 1
    mv -f "$WORK/firnc1.new" "$FC1" || exit 1
fi

same=0
# Per-run timeout in seconds: 20 on an idle machine, scaled up with the load
# (load average per core, at most x6) so that a busy server does not turn
# slow-but-correct programs into false deviations. Override: SELF_COMPARE_TIMEOUT.
run_timeout() {
    if [ -n "${SELF_COMPARE_TIMEOUT:-}" ]; then echo "$SELF_COMPARE_TIMEOUT"; return; fi
    awk -v n="$(nproc)" '{f=$1/n; if (f<1) f=1; if (f>6) f=6; printf "%d", 20*f}' /proc/loadavg
}

different=0
noncore=0
comptime=0
defer_count=0
codegen=0
skipped=0
faulty=0
first=""

while IFS= read -r f; do
    # r664: `// firnc1_skip: <why>` in the first lines = a stage-0-only feature (not yet in firnc1).
    if head -n 3 "$f" | grep -q '^// firnc1_skip'; then
        skipped=$((skipped+1))
        continue
    fi
    # r936: `// self_compare_skip: <why>` = the output holds timings, so two correct builds never print the same.
    if head -n 3 "$f" | grep -q '^// self_compare_skip'; then
        skipped=$((skipped+1))
        continue
    fi
    # Only files that `firnc0` can compile -- otherwise they would be two
    # different inputs. Since round 29 files with `import` count
    # as well: `firnc1` resolves them itself.
    if ! "$FIRNC" "$f" -o "$WORK/ref" 2>/dev/null; then
        if [ ! -x "$FIRNC" ] || [ ! -x "$FC1" ]; then
            echo "ABORT: compiler binary vanished mid-run (worktree or target dir removed): $FIRNC / $FC1"
            exit 2
        fi
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
    tmo=$(run_timeout)
    # The reference run is measured in CPU seconds. A program that is slow by
    # nature (firnc1 emits slower code: 2831 needs 9 s with firnc0 and 68 s with
    # firnc1) gets a budget of 15x the reference CPU time + 20 s, scaled by the
    # same load factor; so neither a busy machine nor a heavy test turns into a
    # false deviation. (r480)
    rm -f "$WORK/ref.cpu"
    /usr/bin/time -f %U -o "$WORK/ref.cpu" timeout "$tmo" "$WORK/ref" > "$WORK/ref.out" 2>/dev/null
    rref=$?
    rcpu=$(tail -1 "$WORK/ref.cpu" 2>/dev/null | awk '{printf "%d", $1}')
    tmo=$(awk -v t="$tmo" -v c="${rcpu:-0}" 'BEGIN{x=(c*15+20)*t/20; if (x<t) x=t; printf "%d", x}')
    timeout "$tmo" "$WORK/a.bin" > "$WORK/a.out" 2>/dev/null
    ra=$?
    if [ "$rref" -eq "$ra" ] && cmp -s "$WORK/ref.out" "$WORK/a.out"; then
        same=$((same+1))
    else
        different=$((different+1))
        [ -z "$first" ] && first="$f (firnc0: $rref, firnc1: $ra)"
    fi
done < <(find tests bench -name '*.fi' -not -type l -not -path 'tests/neg/*' -not -path 'tests/lexneg/*' | sort \
    | awk -v k="$SHARD_K" -v n="$SHARD_N" 'NR % n == k')

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
