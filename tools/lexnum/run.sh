#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/lexnum/run.sh -- THE NUMBER READER OF THE TWO LEXERS, MEASURED
# (round 65).
#
# WHY THIS EXISTS: `tools/lex_compare.sh` runs both lexers over the source
# corpus of the project. That corpus contains a few dozen floating point
# literals, all of them harmless -- which is why the divergence at
# `9007199254740991.0` only came to light in round 63, and only by accident.
# A number reader is not proven by the numbers that happen to be lying
# around; it is proven by the numbers that are MEANT to hurt.
#
# Here, several thousand literals are read by FOUR readers and their BIT
# PATTERNS are held against each other:
#
#   firnc0        the lexer in Rust      compiler/src/lexer.rs
#   firnc1        the lexer in Firn      lib/firnc1/lexer.fi
#   C strtod      glibc                  tools/lexnum/ref.c
#   Python float  CPython                tools/lexnum/check.py
#
# The last two are MEASURING INSTRUMENTS, not dependencies: nothing in the
# compiler uses them. They are there so that "both agree" cannot become
# "both are wrong in the same way".
#
# Additionally the literals that have to be REFUSED are compared -- there
# both streams count, standard output and the diagnostics, exactly as in
# `tools/lex_compare.sh`.
#
# Usage:  bash tools/lexnum/run.sh [COUNT] [SEED]
set -uo pipefail
cd "$(dirname "$0")/../.."

COUNT=${1:-5000}
SEED=${2:-65065}
WORK=.lexnum-work
FIRNC=compiler/target/release/firnc
DUMP=${DUMP:-./.lexdump}
export FIRNLIB="$(pwd)/lib"

mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
# The same rule as in tools/lex_compare.sh: never measure an outdated dump
# binary.
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/lexdump.fi -o "$DUMP" || exit 1
fi
cc -O2 -o "$WORK/ref" tools/lexnum/ref.c || exit 1
cc -O2 -o "$WORK/ref32" tools/lexnum/ref32.c || exit 1

echo "== 1. produce the literals (seed $SEED) =="
python3 tools/lexnum/gen.py "$COUNT" "$SEED" "$WORK" || exit 1

echo "== 2. read them with all four readers =="
"$FIRNC" --emit=tokens "$WORK/float_cases.fi" > "$WORK/float_a.txt" 2>"$WORK/float_ae.txt"
"$DUMP" "$WORK/float_cases.fi" > "$WORK/float_b.txt" 2>"$WORK/float_be.txt"
"$FIRNC" --emit=tokens "$WORK/int_cases.fi" > "$WORK/int_a.txt" 2>"$WORK/int_ae.txt"
"$DUMP" "$WORK/int_cases.fi" > "$WORK/int_b.txt" 2>"$WORK/int_be.txt"
"$WORK/ref" < "$WORK/float_plain.txt" > "$WORK/float_c.txt" || exit 1
# ROUND 71: the same four columns for the single width.
"$FIRNC" --emit=tokens "$WORK/f32_cases.fi" > "$WORK/f32_a.txt" 2>"$WORK/f32_ae.txt"
"$DUMP" "$WORK/f32_cases.fi" > "$WORK/f32_b.txt" 2>"$WORK/f32_be.txt"
"$WORK/ref32" < "$WORK/f32_plain.txt" > "$WORK/f32_c.txt" || exit 1
if [ -s "$WORK/f32_ae.txt" ]; then
    echo "FAIL: firnc0 reports something about the valid f32 literals:"
    head -5 "$WORK/f32_ae.txt"
    exit 1
fi
if grep -q '^error' "$WORK/f32_be.txt"; then
    echo "FAIL: firnc1 reports something about the valid f32 literals:"
    head -5 "$WORK/f32_be.txt"
    exit 1
fi

if [ -s "$WORK/float_ae.txt" ]; then
    echo "FAIL: firnc0 reports something about the valid literals:"
    head -5 "$WORK/float_ae.txt"
    exit 1
fi
if grep -q '^error' "$WORK/float_be.txt"; then
    echo "FAIL: firnc1 reports something about the valid literals:"
    head -5 "$WORK/float_be.txt"
    exit 1
fi

echo "== 3. bit pattern against bit pattern =="
python3 tools/lexnum/check.py "$WORK"
RC=$?

echo "== 4. the literals that have to be REFUSED =="
"$FIRNC" --emit=tokens "$WORK/bad_cases.fi" > "$WORK/bad_a.txt" 2>"$WORK/bad_ae.txt"
"$DUMP" "$WORK/bad_cases.fi" > "$WORK/bad_b.txt" 2>"$WORK/bad_be.txt"
# The dump binary writes its counts to the error output when there was no
# diagnostic; with diagnostics the whole stream belongs to the messages.
BAD=0
if ! cmp -s "$WORK/bad_a.txt" "$WORK/bad_b.txt"; then
    echo "   the token streams differ:"
    diff "$WORK/bad_a.txt" "$WORK/bad_b.txt" | head -10
    BAD=1
fi
if ! cmp -s "$WORK/bad_ae.txt" "$WORK/bad_be.txt"; then
    echo "   the diagnostics differ:"
    diff "$WORK/bad_ae.txt" "$WORK/bad_be.txt" | head -10
    BAD=1
fi
NBAD=$(grep -c '' "$WORK/bad_cases.fi")
NMSG=$(grep -c '^error:' "$WORK/bad_ae.txt")
echo "   refused literals       %s" | sed "s/%s/$NBAD, $NMSG messages, identical: $([ $BAD -eq 0 ] && echo yes || echo NO)/"
if [ "$NMSG" -eq 0 ]; then
    echo "FAIL: not a single message -- the negative cases are not arriving"
    BAD=1
fi

FLOAT_TOTAL=$(grep -c '' "$WORK/float_cases.fi")
SLOW=$(awk '{print $5}' "$WORK/float_be.txt" 2>/dev/null | tail -1)
echo
if [ "$RC" -eq 0 ] && [ "$BAD" -eq 0 ]; then
    F32_TOTAL=$(grep -c '' "$WORK/f32_cases.fi")
    echo "OK: $FLOAT_TOTAL f64 and $F32_TOTAL f32 literals, four readers each, no deviation (exact path: ${SLOW:-?})"
    exit 0
fi
echo "FAIL: the number readers do not agree"
exit 1
