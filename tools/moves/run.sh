#!/usr/bin/env bash
# tools/moves/run.sh -- THE MOVE CHECKER AND `drop`, MEASURED IN BOTH COMPILERS
# (round OWN-3 / r192, compiler/src/moves.rs and lib/firnc1/moves.fi).
#
# WHAT IS PROVEN HERE:
#
#   1. THE REFUSED ONES (tests/neg/move_*.fi, tests/neg/drop_*.fi): every one
#      has to be refused by firnc0 AND by the self-hosted firnc1, and the two
#      messages have to be the same -- the headline, the position, the source
#      line, `= note:`. Marker width included (r196: the parser keeps the width of
#      every operand's first line, ast.expr_len, as Parser::join does in firnc0).
#
#   2. THE ACCEPTED ONES (tests/2004..2007): programs with `drop` and moves
#      that have to build in both compilers, run, and give the same output and
#      the same exit code -- the order in which the destructors run is the
#      whole point of the feature.
#
# The intermediate representation of both lowerings is compared octet for
# octet by tools/fir_compare.sh (section 15 of test.sh) over the same files.
#
# Usage:  bash tools/moves/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FIRNC1=${FIRNC1:-./.firnc1}
WORK=.moves-work
mkdir -p "$WORK"
rm -f "$WORK"/*

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
# an outdated .firnc1 measures yesterday's state (the lesson of round 46)
rebuild=0
[ -x "$FIRNC1" ] || rebuild=1
if [ -x "$FIRNC1" ]; then
    [ "$FIRNC" -nt "$FIRNC1" ] && rebuild=1
    while IFS= read -r q; do
        [ "$q" -nt "$FIRNC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FIRNC1" || { echo "FAIL  building $FIRNC1"; exit 1; }
fi

pass=0
fail=0
same=0
note() { echo "FAIL  $1"; fail=$((fail + 1)); }

# the two renderings are compared as they are, marker width included
norm() { cat "$1"; }

# ---------------------------------------------------------------- refused
for f in tests/neg/move_*.fi tests/neg/drop_*.fi; do
    name=$(basename "$f" .fi)
    hdr=$(head -1 "$f")
    exp=${hdr#*expect_error: }
    pos=${exp%% *}
    msg=${exp#* }

    "$FIRNC" "$f" -o "$WORK/$name.bin" > "$WORK/$name.c0" 2>&1
    rc0=$?
    "$FIRNC1" "$f" -o "$WORK/$name.1.bin" > "$WORK/$name.c1" 2>&1
    rc1=$?

    if [ "$rc0" -eq 0 ]; then note "$name: firnc0 let it through (exit 0)"; continue; fi
    if [ "$rc1" -eq 0 ]; then note "$name: firnc1 let it through (exit 0)"; continue; fi
    if grep -qE 'panicked at|RUST_BACKTRACE' "$WORK/$name.c0" "$WORK/$name.c1"; then
        note "$name: a panic instead of a clean message"
        continue
    fi
    if ! grep -qF ":$pos" "$WORK/$name.c0"; then note "$name: position '$pos' missing (firnc0)"; continue; fi
    if ! grep -qF "$msg" "$WORK/$name.c0"; then note "$name: text '$msg' missing (firnc0)"; continue; fi
    norm "$WORK/$name.c0" > "$WORK/$name.n0"
    norm "$WORK/$name.c1" > "$WORK/$name.n1"
    if ! cmp -s "$WORK/$name.n0" "$WORK/$name.n1"; then
        note "$name: the two compilers say something different"
        diff "$WORK/$name.n0" "$WORK/$name.n1" | head -8 | sed 's/^/        /'
        continue
    fi
    same=$((same + 1))
    pass=$((pass + 1))
done
refused=$pass

# --------------------------------------------------------------- accepted
accepted=0
for f in tests/2004_move_ok.fi tests/2005_drop_order.fi tests/2006_drop_methods.fi \
         tests/2007_drop_error_path.fi tests/2010_*.fi tests/2011_*.fi tests/2012_drop_*.fi; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .fi)
    "$FIRNC" "$f" -o "$WORK/$name.bin" > "$WORK/$name.c0" 2>&1 || { note "$name: firnc0 refused it"; continue; }
    "$FIRNC1" "$f" -o "$WORK/$name.1.bin" > "$WORK/$name.c1" 2>&1 || { note "$name: firnc1 refused it"; head -6 "$WORK/$name.c1"; continue; }
    "$WORK/$name.bin" > "$WORK/$name.o0" 2>&1; r0=$?
    "$WORK/$name.1.bin" > "$WORK/$name.o1" 2>&1; r1=$?
    if [ "$r0" -ne "$r1" ] || ! cmp -s "$WORK/$name.o0" "$WORK/$name.o1"; then
        note "$name: the programs behave differently (exit $r0 / $r1)"
        diff "$WORK/$name.o0" "$WORK/$name.o1" | head -6 | sed 's/^/        /'
        continue
    fi
    accepted=$((accepted + 1))
    pass=$((pass + 1))
done

echo "  cases: refused $refused, accepted $accepted"
echo "  messages identical (marker width included): $same"
echo "PASS: $pass  FAIL: $fail"
[ "$fail" -eq 0 ]
