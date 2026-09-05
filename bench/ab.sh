#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# bench/ab.sh -- A/B comparison of TWO firnc builds on the same programs.
#
# WHY THIS SCRIPT: `bench/run.sh` measures Firn against Rust. The factor there
# fluctuates with the RUST time, however -- by up to 20 % on a shared machine.
# While optimising the compiler this made an improvement look like a
# regression, although the Firn times stayed practically the same. An A/B
# comparison of two Firn builds does not have that problem: the same programs,
# the same machine, one right after the other.
#
# MINIMUM instead of median: interference only ever makes a run slower, never
# faster. The smallest of N runs is therefore the most robust estimator of the
# undisturbed compute time.
#
# Usage:  bash bench/ab.sh <firnc-old> <firnc-new> [runs]
set -uo pipefail
cd "$(dirname "$0")/.."

OLD="${1:?first firnc missing}"
NEW="${2:?second firnc missing}"
LAEUFE="${3:-9}"
WORK=.bench-ab
rm -rf "$WORK"; mkdir -p "$WORK"

beste() {
    local best="" i a b t
    for ((i = 0; i < LAEUFE; i++)); do
        a=$(date +%s.%N)
        "$@" >/dev/null 2>&1
        b=$(date +%s.%N)
        t=$(awk -v a="$a" -v b="$b" 'BEGIN{printf "%.6f", b-a}')
        best=$(awk -v x="$t" -v y="$best" 'BEGIN{if(y==""||x+0<y+0)print x;else print y}')
    done
    echo "$best"
}

printf '%-14s %10s %10s %9s\n' PROGRAM OLD NEW CHANGE
echo "---------------------------------------------------"
summe_alt=0
summe_neu=0
for src in bench/firn/*.fi; do
    name=$(basename "$src" .fi)
    if ! "$OLD" "$src" -o "$WORK/$name.old" 2>"$WORK/$name.old.err"; then
        printf '%-14s   BUILD ERROR (old)\n' "$name"; continue
    fi
    if ! "$NEW" "$src" -o "$WORK/$name.new" 2>"$WORK/$name.new.err"; then
        printf '%-14s   BUILD ERROR (new)\n' "$name"; continue
    fi
    # Same result? Otherwise the measurement is worthless.
    "$WORK/$name.old" > "$WORK/$name.old.out" 2>&1
    "$WORK/$name.new" > "$WORK/$name.new.out" 2>&1
    if ! cmp -s "$WORK/$name.old.out" "$WORK/$name.new.out"; then
        printf '%-14s   RESULT DIFFERS — measurement invalid\n' "$name"
        continue
    fi
    ta=$(beste "$WORK/$name.old")
    tn=$(beste "$WORK/$name.new")
    summe_alt=$(awk -v a="$summe_alt" -v b="$ta" 'BEGIN{print a+b}')
    summe_neu=$(awk -v a="$summe_neu" -v b="$tn" 'BEGIN{print a+b}')
    awk -v n="$name" -v a="$ta" -v b="$tn" \
        'BEGIN{printf "%-14s %9.4fs %9.4fs %+8.1f%%\n", n, a, b, (b-a)/a*100}'
done
echo "---------------------------------------------------"
awk -v a="$summe_alt" -v b="$summe_neu" \
    'BEGIN{printf "%-14s %9.4fs %9.4fs %+8.1f%%\n", "TOTAL", a, b, (b-a)/a*100}'
