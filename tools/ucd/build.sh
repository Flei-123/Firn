#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/ucd/build.sh -- THE BUILD STEP OF ACCEPTANCE ITEM 6 (round 95).
#
# The criterion reads: "a build script reads the Unicode Character Database
# and produces a Firn table from it; the size of the generated table is
# documented." Round 85 showed that the COMPILER can read the real UCD at
# compile time (`tools/ucd/run.sh`), but the result was a dump on standard
# output that nobody used. This script produces the file the item names --
# `generated/unicode_tables.fi` -- and the engine uses it.
#
# Steps:
#   0. the data files are unchanged (sha256 against tools/ucd/UCD.sha256)
#   1. tools/ucd/gen_ucd.fi is up to date (expand_tables.py --check)
#   2. the COMPILER reads the UCD: `firnc --emit=comptime` shows the source
#      text the `comptime` blocks produce, and its size
#   3. compile and run it -- the entries the compiler read, one per line
#   4. tools/ucd/pack.fi packs them into the three stage table
#   5. firnfmt puts the result into canonical shape and CHECKS it, so that
#      tools/fmt/run.sh cannot fail over a generated file
#   6. the size of the table in octets -- the number the item asks for
#   7. --verify: build a second time and compare octet for octet
#
# Usage:  bash tools/ucd/build.sh [--verify] [--fetch]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

FIRNC="$ROOT/compiler/target/release/firnc"
FIRNFMT="$ROOT/.firnfmt"
WORK=.ucd-work
OUT=generated/unicode_tables.fi
UCD=tools/ucd/UnicodeData.txt
DCP=tools/ucd/DerivedCoreProperties.txt
VERIFY=0
FETCH=0
for a in "$@"; do
    [ "$a" = "--verify" ] && VERIFY=1
    [ "$a" = "--fetch" ] && FETCH=1
done

export FIRNLIB="$ROOT/lib"

fail() { echo "FAILED: $*"; exit 1; }

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
mkdir -p "$WORK" generated

echo "== ACCEPTANCE item 6: the build step that makes $OUT =="

# ---------------------------------------------------------------- 0. data
echo
echo "-- 0. the data files are unchanged --"
if [ "$FETCH" = 1 ]; then
    for f in UnicodeData.txt DerivedCoreProperties.txt; do
        curl -sS -o "tools/ucd/$f" \
            "https://www.unicode.org/Public/UCD/latest/ucd/$f" \
            || fail "could not fetch $f"
        echo "   fetched $f"
    done
fi
[ -f "$UCD" ] || fail "$UCD is missing"
[ -f "$DCP" ] || fail "$DCP is missing"
if ! (cd tools/ucd && sha256sum -c UCD.sha256) > "$WORK/sha.txt" 2>&1; then
    cat "$WORK/sha.txt"
    fail "the sha256 sums do not match tools/ucd/UCD.sha256"
fi
sed 's/^/   /' "$WORK/sha.txt"
UBYTES=$(stat -c%s "$UCD")
DBYTES=$(stat -c%s "$DCP")
USHA=$(sha256sum "$UCD" | cut -d' ' -f1)
DSHA=$(sha256sum "$DCP" | cut -d' ' -f1)
UVER=$(head -1 "$DCP" | sed 's/^# DerivedCoreProperties-\(.*\)\.txt$/\1/')
echo "   UnicodeData.txt            $UBYTES octets, $(wc -l < "$UCD") lines"
echo "   DerivedCoreProperties.txt  $DBYTES octets, $(wc -l < "$DCP") lines"
echo "   Unicode version            $UVER"
[ -n "$UVER" ] || fail "the Unicode version could not be read out of $DCP"

# ------------------------------------------------------------- 1. sources
echo
echo "-- 1. the generator source is up to date --"
python3 tools/ucd/expand_tables.py --check || fail "gen_ucd.fi is not up to date"

# ------------------------------------------- 2. what the compiler reads
echo
echo "-- 2. the compiler reads the UCD at compile time --"
t0=$(date +%s%N)
"$FIRNC" --emit=comptime tools/ucd/gen_ucd.fi > "$WORK/emitted.fi" 2> "$WORK/emit.err" \
    || { head -5 "$WORK/emit.err"; fail "--emit=comptime failed"; }
t1=$(date +%s%N)
echo "   emitted Firn source text: $(stat -c%s "$WORK/emitted.fi") octets, $(wc -l < "$WORK/emitted.fi") lines"
echo "   time for the comptime run: $(( (t1 - t0) / 1000000 )) ms"

# ------------------------------------------------- 3. compile and run it
echo
echo "-- 3. the same run compiles the emitted text and runs it --"
t2=$(date +%s%N)
"$FIRNC" tools/ucd/gen_ucd.fi -o "$WORK/gen_ucd" 2> "$WORK/build.err" \
    || { grep -v RWX "$WORK/build.err" | head -10; fail "the build failed"; }
t3=$(date +%s%N)
echo "   compile (comptime + parse + check + codegen): $(( (t3 - t2) / 1000000 )) ms"
"$WORK/gen_ucd" > "$WORK/dump.txt"
rc=$?
[ $rc -eq 0 ] || fail "gen_ucd stopped with $rc (the spot checks in main() are the reason)"
echo "   entries read out of the UCD: $(wc -l < "$WORK/dump.txt")"
printf '     categories %s, ID_Start %s, ID_Continue %s, upper %s, lower %s\n' \
    "$(grep -c '^C ' "$WORK/dump.txt")" "$(grep -c '^S ' "$WORK/dump.txt")" \
    "$(grep -c '^I ' "$WORK/dump.txt")" "$(grep -c '^U ' "$WORK/dump.txt")" \
    "$(grep -c '^L ' "$WORK/dump.txt")"

# --------------------------------------------------------- 4. the packing
echo
echo "-- 4. tools/ucd/pack.fi builds the three stage table --"
"$FIRNC" tools/ucd/pack.fi -o "$WORK/pack" 2> "$WORK/pack.err" \
    || { grep -v RWX "$WORK/pack.err" | head -10; fail "pack.fi does not compile"; }
sed -e "s|@UBYTES@|$UBYTES|" -e "s|@DBYTES@|$DBYTES|" \
    -e "s|@USHA@|$USHA|" -e "s|@DSHA@|$DSHA|" -e "s|@UVER@|$UVER|" \
    tools/ucd/table_head.fi.in > "$WORK/table.fi"
grep -q '@' "$WORK/table.fi" && fail "a placeholder was left in the head"
t4=$(date +%s%N)
"$WORK/pack" < "$WORK/dump.txt" >> "$WORK/table.fi"
rc=$?
t5=$(date +%s%N)
[ $rc -eq 0 ] || fail "pack.fi stopped with $rc"
echo "   time for the packing: $(( (t5 - t4) / 1000000 )) ms"

# ------------------------------------------------------------ 5. firnfmt
echo
echo "-- 5. firnfmt: the generated file is in canonical shape --"
if [ ! -x "$FIRNFMT" ]; then
    "$FIRNC" tools/fmt/firnfmt.fi -o "$FIRNFMT" 2> "$WORK/fmt.err" \
        || { grep -v RWX "$WORK/fmt.err" | head -5; fail "firnfmt does not build"; }
fi
"$FIRNFMT" "$WORK/table.fi" > "$WORK/table_fmt.fi" || fail "firnfmt refused the file"
if ! cmp -s "$WORK/table.fi" "$WORK/table_fmt.fi"; then
    echo "   the packer's output was not canonical -- the formatted one is taken"
    echo "   (difference: $(cmp -l "$WORK/table.fi" "$WORK/table_fmt.fi" 2>/dev/null | wc -l) octets)"
    cp "$WORK/table_fmt.fi" "$WORK/table.fi"
else
    echo "   the packer writes canonical Firn straight away"
fi
"$FIRNFMT" -c "$WORK/table.fi" > /dev/null || fail "firnfmt -c strikes on the result"

# ------------------------------------------------------------- 6. install
if [ "$VERIFY" = 1 ]; then
    echo
    echo "-- 7. reproducibility: the same source state, the same octets --"
    if [ ! -f "$OUT" ]; then
        fail "$OUT does not exist -- run without --verify first"
    fi
    if cmp -s "$WORK/table.fi" "$OUT"; then
        echo "   $(sha256sum "$OUT" | cut -c1-64)"
        echo "   identical, octet for octet ($(stat -c%s "$OUT") octets)"
    else
        cmp "$WORK/table.fi" "$OUT" | head -3
        fail "the second build differs from $OUT"
    fi
else
    cp "$WORK/table.fi" "$OUT"
fi

echo
echo "-- 6. the size of the generated table --"
BYTES=$(awk -F'= ' '/^const BYTES: usize = /{print $2; exit}' "$OUT")
L2=$(awk -F'= ' '/^const L2_BLOCKS: usize = /{print $2; exit}' "$OUT")
L3=$(awk -F'= ' '/^const L3_BLOCKS: usize = /{print $2; exit}' "$OUT")
UPR=$(awk -F'= ' '/^const UP_RUNS: usize = /{print $2; exit}' "$OUT")
LOR=$(awk -F'= ' '/^const LOW_RUNS: usize = /{print $2; exit}' "$OUT")
echo "   the table at run time : $BYTES octets"
echo "     level 1             : 2176 octets (one per 512 code points)"
echo "     level 2             : $L2 blocks x 64 octets = $((L2 * 64)) octets"
echo "     level 3             : $L3 blocks x 16 octets = $((L3 * 16)) octets"
echo "     case mappings       : $UPR + $LOR runs x 8 octets = $(((UPR + LOR) * 8)) octets, plus 2 x 256 octets index"
echo "   the generated source  : $(stat -c%s "$OUT") octets, $(wc -l < "$OUT") lines"
echo "   words written         : $(grep -c '^    w(p, ' "$OUT") of $((BYTES / 8)) (the zero words come from mmap)"

echo
echo "OK: $OUT built out of the Unicode Character Database $UVER."
exit 0
