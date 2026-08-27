#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/ucd/run.sh -- ACCEPTANCE ITEM 6 against the REAL Unicode Character
# Database (round 85).
#
# The criterion of the item is: "a build script reads the Unicode Character
# Database and produces a Firn table from it; the size of the generated table
# is documented." Up to round 84 that was only shown against
# `tests/data/upper_lower.txt` -- eight hand-picked lines in the FORMAT of
# `UnicodeData.txt`. This script does it against the file itself:
# Unicode 17.0.0, 2,198,209 octets, 40,575 lines, all general categories.
#
# Steps:
#   0. the data file is unchanged (sha256 against tools/ucd/UnicodeData.sha256)
#   1. tools/ucd/ucd_real.fi is up to date (expand.py --check)
#   2. `firnc --emit=comptime` -- the generated Firn source text, its SIZE
#   3. compile and run -- the table prints itself
#   4. tools/ucd/verify.py compares it against a parser of its own over ALL
#      1,114,112 code points
#   5. COUNTER-CHECK: one line of the table is falsified on purpose; the
#      verification HAS to strike. A check that cannot fail proves nothing.
#   6. COUNTER-CHECK: the step budget. One `comptime` block cannot manage the
#      whole file (MAX_STEPS = 2,000,000) -- that is measured here, not
#      claimed, because it is the reason the work is split over eight blocks.
#
# Usage:  bash tools/ucd/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
UCD=tools/ucd/UnicodeData.txt

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

fail() { echo "FAILED: $*"; exit 1; }

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

echo "== ACCEPTANCE item 6: comptime code generation against the real UCD =="

# ---------------------------------------------------------------- 0. data
echo
echo "-- 0. the data file is unchanged --"
[ -f "$UCD" ] || fail "$UCD is missing"
if ! sha256sum -c tools/ucd/UnicodeData.sha256 > "$TMPD/sha.txt" 2>&1; then
    cat "$TMPD/sha.txt"
    fail "the sha256 of $UCD does not match tools/ucd/UnicodeData.sha256"
fi
echo "   $(cat "$TMPD/sha.txt")"
echo "   $(stat -c%s "$UCD") octets, $(wc -l < "$UCD") lines"

# ------------------------------------------------------------- 1. source
echo
echo "-- 1. the generator source is up to date --"
python3 tools/ucd/expand.py --check || fail "ucd_real.fi is not up to date"

# ---------------------------------------------------- 2. the emitted text
echo
echo "-- 2. what the comptime blocks produce --"
t0=$(date +%s%N)
"$FIRNC" --emit=comptime tools/ucd/ucd_real.fi > "$TMPD/generated.fi" 2> "$TMPD/emit.err" \
    || { head -5 "$TMPD/emit.err"; fail "--emit=comptime failed"; }
t1=$(date +%s%N)
gen_bytes=$(stat -c%s "$TMPD/generated.fi")
gen_lines=$(wc -l < "$TMPD/generated.fi")
gen_ranges=$(grep -c '^    d(' "$TMPD/generated.fi")
echo "   generated Firn source text: $gen_bytes octets, $gen_lines lines, $gen_ranges ranges"
echo "   time for the comptime run:  $(( (t1 - t0) / 1000000 )) ms"
[ "$gen_ranges" -gt 3000 ] || fail "only $gen_ranges ranges -- that cannot be the whole UCD"

# ------------------------------------------------- 3. compile and run it
echo
echo "-- 3. the same run compiles the generated text and runs it --"
t2=$(date +%s%N)
"$FIRNC" tools/ucd/ucd_real.fi -o "$TMPD/ucd_real" 2> "$TMPD/build.err" \
    || { head -10 "$TMPD/build.err"; fail "the build failed"; }
t3=$(date +%s%N)
echo "   compile (comptime + parse + check + codegen): $(( (t3 - t2) / 1000000 )) ms"
echo "   binary: $(stat -c%s "$TMPD/ucd_real") octets"
"$TMPD/ucd_real" > "$TMPD/dump.txt"
rc=$?
[ $rc -eq 0 ] || fail "the program stopped with $rc (the spot checks in main() are the reason)"
echo "   the spot checks in main() pass, the table has $(wc -l < "$TMPD/dump.txt") ranges"

# --------------------------------------------------------- 4. the compare
echo
echo "-- 4. against an independent parser, over every code point --"
python3 tools/ucd/verify.py "$TMPD/dump.txt" "$UCD" || fail "the generated table does not match the UCD"

# --------------------------------------------------- 5. counter-check (a)
echo
echo "-- 5. counter-check: a falsified line has to be found --"
awk 'NR==2000 {print $1, $2, "Zz"; next} {print}' "$TMPD/dump.txt" > "$TMPD/broken.txt"
if python3 tools/ucd/verify.py "$TMPD/broken.txt" "$UCD" > "$TMPD/broken.out" 2>&1; then
    cat "$TMPD/broken.out"
    fail "the verification does NOT strike on a falsified table -- it is worthless"
fi
echo "   the verification strikes:"
grep -E 'DIFFERENT|U\+' "$TMPD/broken.out" | head -3 | sed 's/^/   /'

# --------------------------------------------------- 6. counter-check (b)
echo
echo "-- 6. counter-check: one block cannot manage the whole file --"
sed 's/WINDOW/2198209/' tools/ucd/probe_budget.fi.in > "$TMPD/whole.fi"
cp "$UCD" "$TMPD/UnicodeData.txt"
if "$FIRNC" --emit=comptime "$TMPD/whole.fi" > "$TMPD/whole.out" 2> "$TMPD/whole.err"; then
    fail "a single comptime block managed 2,198,209 octets -- then the split into eight blocks is pointless and ucd_real.fi should be simplified"
fi
grep -o 'more than 2000000 steps' "$TMPD/whole.err" | head -1 | sed 's/^/   the compiler says: /'
sed 's/WINDOW/400065/' tools/ucd/probe_budget.fi.in > "$TMPD/part.fi"
"$FIRNC" --emit=comptime "$TMPD/part.fi" > "$TMPD/part.out" 2> "$TMPD/part.err" \
    || { head -3 "$TMPD/part.err"; fail "even 400,065 octets do not go through -- the measurement of the budget is off"; }
echo "   400,065 octets in ONE block go through: $(tr '\n' ' ' < "$TMPD/part.out")"

echo
echo "OK: item 6 -- the compiler read the real UCD (Unicode 17.0.0) at compile"
echo "    time and produced a table of $gen_ranges ranges / $gen_bytes octets of Firn"
echo "    source text from it; it matches the UCD in all 1,114,112 code points."
exit 0
