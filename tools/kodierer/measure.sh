#!/usr/bin/env bash
# ROUND KODIERER, PART 5 -- what the encoder brings AT ONCE.
#
# Question a) How much faster does compiling get without `as`?
#             The study measured "as + ld" at 195.1 us per function. How much
#             of that disappears?
#
# Method: the same file, the same build stage, once through `as`, once through
# the own encoder. BEST VALUE of several runs, so that foreign load on the host
# is not measured along.
#
# Usage: bash tools/kodierer/measure.sh <file.fi> [<file.fi> ...]
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
RUNS="${RUNS:-5}"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT

best() { # $@ = command -> best time in milliseconds
    local b=999999999
    for _ in $(seq "$RUNS"); do
        local t0 t1 d
        t0=$(date +%s%N)
        "$@" > /dev/null 2>&1
        t1=$(date +%s%N)
        d=$(( (t1 - t0) / 1000000 ))
        [ "$d" -lt "$b" ] && b=$d
    done
    echo "$b"
}

echo "======================================================================"
echo "PART 5a -- COMPILE TIME WITH AND WITHOUT THE EXTERNAL ASSEMBLER"
echo "  best of $RUNS runs, milliseconds for the WHOLE file"
echo "======================================================================"
printf "%-34s %10s %10s %10s %8s\n" "Source" "with as" "without" "saving" "factor"

total_old=0
total_new=0
for f in "$@"; do
    [ -f "$f" ] || continue
    a=$(best "$FIRNC" --asm-as -c -o "$W/o1.o" "$f")
    b=$(best "$FIRNC" --asm-intern -c -o "$W/o2.o" "$f")
    diff=$((a - b))
    fac=$(python3 -c "print('%.2fx' % ($a / max($b,1)))")
    printf "%-34s %10s %10s %10s %8s\n" "$(basename "$f")" "$a" "$b" "$diff" "$fac"
    total_old=$((total_old + a))
    total_new=$((total_new + b))
done
echo "----------------------------------------------------------------------"
printf "%-34s %10s %10s %10s\n" "SUM" "$total_old" "$total_new" "$((total_old - total_new))"
python3 -c "
old=$total_old; new=$total_new
print('  The external assembler cost %.1f %% of the compile time.' % (100.0*(old-new)/max(old,1)))
print('  Without it firnc compiles %.2f times as fast.' % (old/max(new,1)))
"
