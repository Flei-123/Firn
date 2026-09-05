#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# bench/instr.sh -- A/B comparison of two firnc builds over the EXECUTED
# INSTRUCTIONS instead of over the clock.
#
# WHY: on this machine the wall-clock time of the same binary fluctuates by up
# to 40 % between runs -- measured, not assumed. That makes it impossible to
# judge a 5 % codegen change; on the first attempt the very same improvement
# showed up once as -18 % and once as +6 %.
#
# `valgrind --tool=callgrind` counts the instructions actually executed. The
# result is reproducible down to the single instruction.
#
# HONEST LIMIT: the instruction count is NOT the run time. It says nothing
# about cache misses, branch prediction or dependency chains; an `lea` and a
# `div` both count as one instruction. It is the right metric for the question
# "does the compiler emit less work?" -- and it is used here for that alone.
# The clock stays responsible for the final result
# (`bench/run.sh`, `bench/ab.sh`).
#
# Usage:  bash bench/instr.sh <firnc-old> <firnc-new> [program ...]
set -uo pipefail
cd "$(dirname "$0")/.."

OLD="${1:?first firnc missing}"
NEW="${2:?second firnc missing}"
shift 2
WORK=.bench-instr
mkdir -p "$WORK"

QUELLEN=()
if [ "$#" -gt 0 ]; then
    for n in "$@"; do QUELLEN+=("bench/firn/$n.fi"); done
else
    for f in bench/firn/*.fi; do QUELLEN+=("$f"); done
fi

# Count the instructions of one program. Empty output = failed.
zaehle() {
    local bin="$1" tag="$2"
    if ! timeout 900 valgrind --tool=callgrind \
        --callgrind-out-file="$WORK/$tag.cg" "$bin" >/dev/null 2>"$WORK/$tag.log"; then
        return 1
    fi
    grep -oP 'I\s+refs:\s+\K[0-9,]+' "$WORK/$tag.log" | tr -d ','
}

printf '%-14s %16s %16s %10s\n' PROGRAM OLD NEW CHANGE
echo "-------------------------------------------------------------"
sa=0
sn=0
for src in "${QUELLEN[@]}"; do
    name=$(basename "$src" .fi)
    if ! "$OLD" "$src" -o "$WORK/$name.old" 2>/dev/null; then
        printf '%-14s   BUILD ERROR (old)\n' "$name"
        continue
    fi
    if ! "$NEW" "$src" -o "$WORK/$name.new" 2>/dev/null; then
        printf '%-14s   BUILD ERROR (new)\n' "$name"
        continue
    fi
    a=$(zaehle "$WORK/$name.old" "$name.old")
    n=$(zaehle "$WORK/$name.new" "$name.new")
    if [ -z "$a" ] || [ -z "$n" ]; then
        printf '%-14s   NO COUNT (valgrind)\n' "$name"
        continue
    fi
    sa=$(awk -v x="$sa" -v y="$a" 'BEGIN{printf "%.0f", x+y}')
    sn=$(awk -v x="$sn" -v y="$n" 'BEGIN{printf "%.0f", x+y}')
    awk -v x="$name" -v a="$a" -v b="$n" \
        'BEGIN{printf "%-14s %16d %16d %+9.2f%%\n", x, a, b, (b-a)/a*100}'
done
echo "-------------------------------------------------------------"
awk -v a="$sa" -v b="$sn" \
    'BEGIN{if(a>0) printf "%-14s %16d %16d %+9.2f%%\n", "TOTAL", a, b, (b-a)/a*100}'
