#!/usr/bin/env bash
# tools/fir_compare.sh -- the lowering in FIRN against the one in RUST.
#
# The YARDSTICK is `firnc0 --emit=fir-raw`: the intermediate representation DIRECTLY after
# the lowering, without any optimisation. What is compared is the text octet for octet,
# and it contains everything that matters: value numbers, block numbers,
# the order of the instructions, terminators.
#
# That is the sharpest comparison of the whole series. Two value numbers in
# a different order, one block too many or too few -- and the text no longer
# matches.
#
# Return values of `.firdump`:
#   0 output * 1 error * 3 not core language * 4 comptime needed *
#   5 `defer`/`errdefer` (not ported in the lowering yet)
#
# ONLY WHAT `firnc0` can compile SEPARATELY is tested: `--emit=fir-raw`
# otherwise runs over the merged module program, while the Firn way goes
# over a single file -- that would be no comparison but two
# different inputs.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${FIRDUMP:-./.firdump}

# Rebuild when the dump binary is missing OR sources are younger
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/firdump.fi -o "$DUMP" || exit 1
fi

# KNOWN DEVIATIONS -- each one named separately, with a reason.
#
#   tests/590_f64.fi  ->  the literal `1e308`, the floating point rounding
#   case of round 20 (see tools/lex_compare.sh). The value is already wrong
#   in the token, not in the lowering.
#
#   tests/871_closure_plain.fi  ->  ROUND 58, order and numbering of the
#   GENERATED closure functions. `firnc0` appends all `__closure#N` at the
#   END of the module and numbers them from 0; `firnc1` emits them where
#   they appear and numbers them from 2. The bodies are the same and the
#   BEHAVIOUR is the same -- tools/self_compare.sh compares that and finds
#   no difference. What differs is a name and a position in the text, not a
#   program. Found in round 64 while reviving this script (it had been
#   calling `--emit=typen` since the English migration, an option that no
#   longer exists -- so it compared NOTHING and reported zeros).
KNOWN="tests/590_f64.fi tests/871_closure_plain.fi"

# IMPORTS (r752/r765). `firnc0 --emit=fir-raw` lowers the whole merged module program: every function of
# every imported module, used or not. `.firdump` has no module loader and lowers the one file. A file
# with an `import` line (even an unused one) therefore compares two different inputs, like the
# note at the top says. By default such files are counted under IMPORTS and not compared; cross-module lowering
# is covered by tools/fixpoint.sh, tools/self_compare.sh and tools/prune_compare.sh (real compiles).
# OPT-IN (r765): `FIR_IMPORTS_STEP=N` compares every N-th such file (N=1: all, ~5-20 s each) against
# `firnc1` with `FIRN_EMIT_FIR_RAW=1` (the real module loader) through tools/fir_norm.py (function order, build
# path, @text numbering normalised; two named runtime functions ignored). The result goes to the extra lines
# IMPORTS SAME / IMPORTS DIFFERENT and NEVER changes the exit code. Known rest: programs that use `gc` allocation
# expressions (r767).
IMPORTS_STEP=${FIR_IMPORTS_STEP:-0}
FIRNC1=""
if [ "$IMPORTS_STEP" -gt 0 ]; then
    FIRNC1="$TMPD"/firnc1
    FIRNLIB=lib "$FIRNC" bin/firnc1.fi -o "$FIRNC1" || exit 1
fi
imp_same=0
imp_diff=0
imp_first=""

same=0
different=0
known=0
noncore=0
comptime=0
imports=0
defer_count=0
skipped=0
instructions=0
first=""

while IFS= read -r f; do
    # r664: `// firnc1_skip: <why>` in the first lines = a stage-0-only feature (not yet in firnc1).
    if head -n 3 "$f" | grep -q '^// firnc1_skip'; then
        skipped=$((skipped+1))
        continue
    fi
    if grep -qE '^[[:space:]]*import[[:space:]]' "$f"; then
        imports=$((imports+1))
        if [ "$IMPORTS_STEP" -gt 0 ] && [ $((imports % IMPORTS_STEP)) -eq 0 ]; then
            if "$FIRNC" --emit=fir-raw "$f" > "$TMPD"/imp_a.txt 2>/dev/null \
                && FIRN_EMIT_FIR_RAW=1 FIRNLIB=lib "$FIRNC1" "$f" -o "$TMPD"/imp_x > "$TMPD"/imp_b.txt 2>/dev/null; then
                if why=$(python3 tools/fir_norm.py "$TMPD"/imp_a.txt "$TMPD"/imp_b.txt); then
                    imp_same=$((imp_same+1))
                else
                    imp_diff=$((imp_diff+1))
                    [ -z "$imp_first" ] && imp_first="$f: $why"
                fi
            fi
        fi
        continue
    fi
    if ! "$FIRNC" --emit=types "$f" >/dev/null 2>&1; then
        skipped=$((skipped+1))
        continue
    fi
    if ! "$FIRNC" --emit=fir-raw "$f" > "$TMPD"/firv_a.txt 2>/dev/null; then
        skipped=$((skipped+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/firv_b.txt 2>/dev/null
    rc=$?
    case "$rc" in
        3) noncore=$((noncore+1)); continue;;
        4) comptime=$((comptime+1)); continue;;
        5) defer_count=$((defer_count+1)); continue;;
    esac
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/firv_a.txt "$TMPD"/firv_b.txt; then
        same=$((same+1))
        n=$(grep -c '^  ' "$TMPD"/firv_a.txt)
        instructions=$((instructions + n))
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
echo "INSTRUCTIONS:  $instructions  (value numbers and blocks included)"
echo "DEFER:         $defer_count  (not ported in the lowering yet)"
echo "IMPORTS:       $imports  (module program, not comparable one file at a time)"
if [ "$IMPORTS_STEP" -gt 0 ]; then
    echo "IMPORTS SAME:      $imp_same  (every $IMPORTS_STEP-th file, firnc1 module loader, normalised)"
    echo "IMPORTS DIFFERENT: $imp_diff  (informational; first: ${imp_first:-none})"
fi
echo "NOT CORE:      $noncore"
echo "COMPTIME:      $comptime"
echo "SKIPPED:       $skipped  (firnc0 does not compile the file on its own)"
if [ -n "$first" ]; then
    echo "first unexpected deviation: $first"
    ff=${first%% *}
    diff <("$FIRNC" --emit=fir-raw "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -12
    exit 1
fi
exit 0
