#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/sema_compare.sh -- the type checker in FIRN against the one in RUST.
#
# The YARDSTICK is `firnc0 --emit=types`: the canonical syntax tree with the TYPE at
# every expression. `firnc0` promises that after the check every expression
# has a concrete type -- exactly this promise is compared here.
#
# Return values of `.semadump`:
#   0  output produced
#   1  error
#   3  not core language (enum/match, error unions, generics, gc, attributes,
#      comptime, the intrinsics for constant run time)
#   4  a constant needs evaluation at compile time (comptime.rs)
#
# SKIPPED are files that `firnc0` ITSELF cannot check separately
# -- almost all of them import a module whose names are unknown on their own.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${SEMADUMP:-./.semadump}

if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/semadump.fi -o "$DUMP" || exit 1
fi

# KNOWN DEVIATION: tests/590_f64.fi, the literal `1e308`. No type error
# but the floating point rounding case from round 20 -- the value is already
# wrong in the token.
KNOWN="tests/590_f64.fi"

same=0
different=0
known=0
noncore=0
comptime=0
skipped=0
exprs=0
first=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=types "$f" > "$TMPD"/semv_a.txt 2>/dev/null; then
        skipped=$((skipped+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/semv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        noncore=$((noncore+1))
        continue
    fi
    if [ "$rc" -eq 4 ]; then
        comptime=$((comptime+1))
        continue
    fi
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/semv_a.txt "$TMPD"/semv_b.txt; then
        same=$((same+1))
        # Every " :" is a typed expression.
        n=$(grep -o ' :' "$TMPD"/semv_a.txt | wc -l)
        exprs=$((exprs + n))
        continue
    fi
    different=$((different+1))
    if echo "$KNOWN" | tr ' ' '\n' | grep -qxF "$f"; then
        known=$((known+1))
    else
        [ -z "$first" ] && first="$f (rc=$rc)"
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "SAME:          $same"
echo "DIFFERENT:     $different   (known and named: $known)"
echo "EXPRESSIONS:   $exprs  (each with the same type as in firnc0)"
echo "NOT CORE:      $noncore"
echo "COMPTIME:      $comptime  (constant evaluation at compile time, not ported)"
echo "SKIPPED:       $skipped  (firnc0 does not check the file on its own)"
if [ -n "$first" ]; then
    echo "first unexpected deviation: $first"
    ff=${first%% *}
    diff <("$FIRNC" --emit=types "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -6
    exit 1
fi
exit 0
