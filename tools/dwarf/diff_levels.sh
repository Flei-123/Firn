#!/usr/bin/env bash
# tools/dwarf/diff_levels.sh -- THE DEBUGGER AS A TEST OF THE OPTIMIZER
# (round 96).
#
# The variable information of round 96 makes a question askable that could
# not be asked before: does a program compute THE SAME THING with the
# optimizer as without it -- not at the end, where the exit code says so
# anyway, but IN THE MIDDLE?
#
# The method: build the same file twice (`--no-opt -g` and
# `--opt-level=dev-fast -g`), stop at the same function in both, and hold
# the PARAMETERS against each other. A parameter that is missing in the
# optimized build is the price of the optimization and is counted. One that
# is THERE in both and shows a DIFFERENT value is a bug -- either in the
# optimizer or in the debug information, and both are ours.
#
# WHY ONLY THE PARAMETERS, and this cost the first run of the round: `break
# <function>` stops AFTER the prologue and BEFORE the first statement. At
# that point the local variables hold whatever was in their storage --
# garbage, and legitimately DIFFERENT garbage in two builds whose frames
# look nothing alike. Comparing them reported three "differences" in
# `examples/tour.fi` that were nothing of the sort. A parameter, on the
# other hand, has an exactly defined value at that very point: the one the
# caller passed. It is the only thing at a function entry that two builds
# owe each other.
#
# Usage:  bash tools/dwarf/diff_levels.sh [file.fi ...]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FILES=${*:-}
if [ -z "$FILES" ]; then
    FILES=$(ls tests/0*.fi tests/1[0-9][0-9]_*.fi tools/dwarf/probe.fi docs/gdb_example.fi 2>/dev/null)
fi

pairs=0
same=0
addr=0
differ=0
gone=0
skipped=0
first=""

for f in $FILES; do
    rm -f "$TMPD/a" "$TMPD/b"
    "$FIRNC" --no-opt -g -o "$TMPD/a" "$f" > /dev/null 2>&1 || { skipped=$((skipped + 1)); continue; }
    "$FIRNC" --opt-level=dev-fast -g -o "$TMPD/b" "$f" > /dev/null 2>&1 || { skipped=$((skipped + 1)); continue; }
    # Every function of the file that the debug information knows.
    fns=$(readelf --debug-dump=info "$TMPD/a" 2>/dev/null |
        grep -A 3 DW_TAG_subprogram | grep DW_AT_name | sed 's/.*: //' | tr -d ' ')
    for fn in $fns; do
        [ "$fn" = "main" ] && continue
        for side in a b; do
            gdb -batch -ex "break $fn" -ex run -ex "info args" -ex kill \
                "$TMPD/$side" 2>/dev/null |
                sed -n '/^Breakpoint 1,/,$p' |
                grep -E '^[A-Za-z_][A-Za-z0-9_]* = ' | sort > "$TMPD/$side.vars"
        done
        [ -s "$TMPD/a.vars" ] || continue
        pairs=$((pairs + 1))
        while IFS= read -r line; do
            name=${line%% = *}
            want=${line#* = }
            got=$(grep "^$name = " "$TMPD/b.vars" | head -1)
            got=${got#* = }
            if [ -z "$got" ] || [ "$got" = "<optimized out>" ]; then
                gone=$((gone + 1))
                continue
            fi
            # AN ADDRESS IS NOT A VALUE. A pointer to a local names a frame
            # offset, and two builds whose frames look nothing alike have no
            # reason to hand out the same one -- `examples/tour.fi` reported
            # four such "differences" and every one of them was a pointer
            # into the caller's frame. Anything that looks like an address is
            # counted separately instead of being compared: `0x...`, a struct
            # that holds one, and a number in the stack range.
            case "$want$got" in
                *0x*)
                    addr=$((addr + 1))
                    continue
                    ;;
                *) ;;
            esac
            case "$want" in
                [0-9]*)
                    # 0x400000 is where this linker puts the program. Any
                    # number at or above it CAN be an address -- of a static,
                    # of a function, of a frame -- and two builds have no
                    # reason to agree on one. `__gc_alloc_in(st = 4831632)`
                    # against `4647520` is the state block of the collector,
                    # not a difference in what the program computes. Numbers
                    # below it are compared; that is where the values a
                    # program really works with live.
                    if [ "$want" -ge 4194304 ] 2> /dev/null; then
                        addr=$((addr + 1))
                        continue
                    fi
                    ;;
                *) ;;
            esac
            if [ "$got" = "$want" ]; then
                same=$((same + 1))
            else
                differ=$((differ + 1))
                [ -z "$first" ] && first="$f:$fn:$name  --no-opt='$want'  dev-fast='$got'"
                echo "  DIFFERENT  $f  $fn  $name: --no-opt='$want' dev-fast='$got'"
            fi
        done < "$TMPD/a.vars"
    done
done

echo
echo "pairs of breakpoints: $pairs   parameters equal: $same   DIFFERENT: $differ   addresses (not comparable): $addr   gone with the optimizer: $gone   files skipped: $skipped"
[ -n "$first" ] && echo "first difference: $first"
[ "$differ" -eq 0 ]
