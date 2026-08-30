#!/usr/bin/env bash
# tools/bootstrap_gaps.sh -- WHAT CAN firnc0 THAT firnc1 CANNOT?
#
# `tools/self_compare.sh` answers one half of the question: of the programs
# that BOTH compilers accept, do they behave the same? It deliberately
# leaves out `tests/neg/` and `tests/lexneg/` -- the programs that have to
# be REFUSED. That is the more dangerous half: a compiler that accepts a
# broken program produces silent wrong code, and a compiler that refuses a
# good one without saying why is nearly as bad.
#
# This script measures BOTH halves and writes a machine readable table.
#
#   positive corpus  tests/ examples/ bench/ (minus neg, lexneg)
#     firnc0 compiles + runs  ->  exit code and standard output
#     firnc1 compiles + runs  ->  the same, or a category:
#       3 not core * 4 comptime * 5 defer * 6 codegen
#       OTHER  = a gap: firnc0 can do it, firnc1 cannot
#       DIFFER = the worst case: both compile, the programs disagree
#
#   negative corpus  tests/neg/ tests/lexneg/
#     firnc0 refuses with a message and a position (that is what is tested).
#     firnc1: REFUSED_LOUD  = non-zero and says something
#             REFUSED_MUTE  = non-zero and says NOTHING
#             ACCEPTED      = compiles a program that is not legal Firn
#
# Usage:  bash tools/bootstrap_gaps.sh [OUT.tsv]   (default .gaps.tsv)
# Env:    FIRNC1=./.firnc2   to measure a different stage
#         JOBS=12
set -uo pipefail
cd "$(dirname "$0")/.."

export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC0:-compiler/target/release/firnc}
FC1=${FIRNC1:-./.firnc1}
OUT=${1:-.gaps.tsv}
JOBS=${JOBS:-$(nproc)}

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
[ -x "$FC1" ]   || { echo "firnc1 is missing: $FC1";   exit 1; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# One file, one line of the table. Runs in a subshell of its own so that
# several may run at once; every file gets its own working directory.
one() {
    local f="$1" kind="$2" w
    w="$WORK/$(echo "$f" | tr '/' '_')"
    mkdir -p "$w"
    local rc0 rc1 r0 r1 msg
    "$FIRNC" "$f" -o "$w/ref" > "$w/c0.txt" 2>&1
    rc0=$?
    "$FC1" "$f" -o "$w/a.bin" > "$w/c1.txt" 2>&1
    rc1=$?
    if [ "$kind" = neg ]; then
        # firnc0 has to refuse -- that is what tests/neg means.
        if [ "$rc0" -eq 0 ]; then
            printf '%s\tneg\tFIRNC0_ACCEPTS\t%d\t%d\t-\n' "$f" "$rc0" "$rc1"
            return
        fi
        if [ "$rc1" -eq 0 ]; then
            printf '%s\tneg\tACCEPTED\t%d\t%d\t-\n' "$f" "$rc0" "$rc1"
            return
        fi
        if [ -s "$w/c1.txt" ]; then
            printf '%s\tneg\tREFUSED_LOUD\t%d\t%d\t-\n' "$f" "$rc0" "$rc1"
        else
            printf '%s\tneg\tREFUSED_MUTE\t%d\t%d\t-\n' "$f" "$rc0" "$rc1"
        fi
        return
    fi
    # positive corpus
    if [ "$rc0" -ne 0 ]; then
        printf '%s\tpos\tSKIPPED\t%d\t%d\t-\n' "$f" "$rc0" "$rc1"
        return
    fi
    case "$rc1" in
        3) printf '%s\tpos\tNOT_CORE\t0\t3\t-\n' "$f"; return;;
        4) printf '%s\tpos\tCOMPTIME\t0\t4\t-\n' "$f"; return;;
        5) printf '%s\tpos\tDEFER\t0\t5\t-\n' "$f"; return;;
        6) printf '%s\tpos\tCODEGEN\t0\t6\t-\n' "$f"; return;;
    esac
    if [ "$rc1" -ne 0 ] || [ ! -x "$w/a.bin" ]; then
        msg=mute
        [ -s "$w/c1.txt" ] && msg=loud
        printf '%s\tpos\tGAP_%s\t0\t%d\t%s\n' "$f" "$(echo "$msg" | tr a-z A-Z)" "$rc1" \
            "$(head -c 120 "$w/c1.txt" | tr '\n\t' '  ')"
        return
    fi
    timeout 30 "$w/ref"   > "$w/ref.out" 2>/dev/null; r0=$?
    timeout 30 "$w/a.bin" > "$w/a.out"   2>/dev/null; r1=$?
    if [ "$r0" -eq "$r1" ] && cmp -s "$w/ref.out" "$w/a.out"; then
        printf '%s\tpos\tSAME\t%d\t%d\t-\n' "$f" "$r0" "$r1"
    else
        printf '%s\tpos\tDIFFER\t%d\t%d\t-\n' "$f" "$r0" "$r1"
    fi
}
export -f one
export FIRNC FC1 WORK

{
    find tests bench examples -name '*.fi' -not -type l \
        -not -path 'tests/neg/*' -not -path 'tests/lexneg/*' | sort | \
        xargs -P "$JOBS" -I{} bash -c 'one "$@"' _ {} pos
    find tests/neg tests/lexneg -name '*.fi' -not -type l | sort | \
        xargs -P "$JOBS" -I{} bash -c 'one "$@"' _ {} neg
} > "$OUT"

echo "table: $OUT   ($(wc -l < "$OUT") files)"
echo
echo "--- positive corpus (firnc0 compiles it)"
awk -F'\t' '$2=="pos"{c[$3]++} END{for(k in c) printf "  %-14s %5d\n", k, c[k]}' "$OUT" | sort
echo "--- negative corpus (firnc0 refuses it)"
awk -F'\t' '$2=="neg"{c[$3]++} END{for(k in c) printf "  %-14s %5d\n", k, c[k]}' "$OUT" | sort
echo
echo "--- gaps and deviations in detail"
awk -F'\t' '$3 ~ /^GAP|DIFFER|^ACCEPTED|FIRNC0_ACCEPTS/{printf "  %-12s %-52s rc1=%s %s\n", $3, $1, $5, $6}' "$OUT" | head -80
