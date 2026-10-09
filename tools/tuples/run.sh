#!/usr/bin/env bash
# tools/tuples/run.sh -- tuples (`(A, B)`, `(a, b)`, `t.0`, `let (a, b) = f()`,
# `-> (A, B)`; docs/GAPS.md B5, docs/TUPLES.md) in BOTH compilers.
#
# `firnc0` (Rust) reads the syntax in compiler/src/parser.rs and makes the
# struct of a tuple in compiler/src/sema.rs; `firnc1` (Firn) does the same in
# lib/firnc1/parser.fi, types.fi and sema.fi. A feature that only one of the
# two has breaks the fixpoint the moment anybody uses it, so:
#
#   1. every positive program gives the same exit code and output in both
#   2. the same on the reference programs of this directory (a tuple that
#      holds a struct of a module, the swap that reads its own target)
#   3. every refused program (tests/neg/241*) is refused by both with a real
#      error (exit 1 or 5), never "not core" (3) and never a crash
#   4. the hidden bindings of `let (a, b)` carry the same names in the two
#      syntax trees (`--emit=ast-canon` against `bin/astdump.fi`)
#   5. random programs full of tuples (tools/tuples/gen.py: tuples of every
#      integer width, floats, bool, nested tuples and a struct as parameters
#      and results, up to five elements, folded into one checksum) print the
#      same number from firnc1, from firnc0 on all four build levels and from
#      firnc0 for aarch64 under qemu -- a tuple passed or returned the wrong
#      way (register class, alignment, the hidden result pointer) changes it
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
DUMP=${ASTDUMP:-./.astdump}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
rebuild=0
[ -x "$FC1" ] || rebuild=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && rebuild=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1
fi
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/astdump.fi -o "$DUMP" || exit 1
fi

fail=0
n=0
say() { printf '  %-62s %s\n' "$1" "$2"; }

both() { # name  source
    local name="$1" src="$2"
    "$FIRNC" -o "$WORK/a0" "$src" > "$WORK/a0.err" 2>&1 || { say "$name" "firnc0 FAILED"; head -5 "$WORK/a0.err"; fail=$((fail+1)); return; }
    "$FC1" "$src" -o "$WORK/a1" > "$WORK/a1.err" 2>&1 || { say "$name" "firnc1 FAILED"; head -5 "$WORK/a1.err"; fail=$((fail+1)); return; }
    "$WORK/a0" > "$WORK/a0.out" 2>&1; r0=$?
    "$WORK/a1" > "$WORK/a1.out" 2>&1; r1=$?
    n=$((n+1))
    if [ "$r0" = "$r1" ] && [ "$r0" = "0" ] && cmp -s "$WORK/a0.out" "$WORK/a1.out"; then
        say "$name" "OK (firnc0 and firnc1 agree)"
    else
        say "$name" "DIFFERENT (firnc0 exit $r0, firnc1 exit $r1)"
        fail=$((fail+1))
    fi
    rm -f "$WORK/a0" "$WORK/a1"
}

for src in tests/2410_tuple_basic.fi tests/2411_tuple_abi.fi tests/2412_tuple_drop.fi \
           tests/2413_tuple_swap_alias.fi tests/2414_tuple_module.fi tests/2415_tuple_generic.fi; do
    both "positive: $src" "$src"
done

# the same programs with the optimiser off in firnc0 -- firnc1 has none, so it
# is the yardstick for "does not depend on a pass"
for src in tests/2410_tuple_basic.fi tests/2411_tuple_abi.fi; do
    "$FIRNC" --no-opt -o "$WORK/b0" "$src" > /dev/null 2>&1 && "$WORK/b0" > /dev/null 2>&1; r=$?
    n=$((n+1))
    if [ "$r" = "0" ]; then say "no-opt: $src" "OK"; else say "no-opt: $src" "exit $r"; fail=$((fail+1)); fi
    rm -f "$WORK/b0"
done

for f in tests/neg/241*.fi; do
    "$FIRNC" -o "$WORK/n0" "$f" > "$WORK/n0.err" 2>&1; r0=$?
    "$FC1" "$f" -o "$WORK/n1" > "$WORK/n1.err" 2>&1; r1=$?
    n=$((n+1))
    ok0=0; ok1=0
    [ "$r0" = "1" ] && ok0=1
    case "$r1" in 1|5) ok1=1 ;; esac
    if grep -qE "panicked at|RUST_BACKTRACE" "$WORK/n0.err"; then ok0=0; fi
    if [ "$ok0" = "1" ] && [ "$ok1" = "1" ]; then
        say "refused: $f" "OK (firnc0 $r0, firnc1 $r1)"
    else
        say "refused: $f" "WRONG (firnc0 exit $r0, firnc1 exit $r1)"
        fail=$((fail+1))
    fi
done

# the syntax trees: the hidden names of `let (a, b)` and the literals
for src in tests/2410_tuple_basic.fi tests/2411_tuple_abi.fi tests/2412_tuple_drop.fi tests/2413_tuple_swap_alias.fi; do
    "$FIRNC" --emit=ast-canon "$src" > "$WORK/t0.txt" 2>/dev/null
    "$DUMP" "$src" > "$WORK/t1.txt" 2>/dev/null; rd=$?
    n=$((n+1))
    if [ "$rd" = "0" ] && cmp -s "$WORK/t0.txt" "$WORK/t1.txt"; then
        say "same syntax tree: $src" "OK ($(wc -l < "$WORK/t0.txt") lines)"
    else
        say "same syntax tree: $src" "DIFFERENT (astdump exit $rd)"
        diff "$WORK/t0.txt" "$WORK/t1.txt" | head -6
        fail=$((fail+1))
    fi
done

# random programs: the checksum has to be the same everywhere
SEEDS=${TUPLE_FUZZ:-24}
fz_bad=0
for seed in $(seq 1 "$SEEDS"); do
    python3 tools/tuples/gen.py "$seed" > "$WORK/fz.fi"
    ref=""
    for flags in "--opt-level=release-fast" "--no-opt" "--opt-level=dev-fast" "--opt-level=release-safe"; do
        if ! "$FIRNC" $flags -o "$WORK/fz0" "$WORK/fz.fi" > "$WORK/fz.err" 2>&1; then
            say "fuzz seed $seed [$flags]" "firnc0 FAILED"; head -3 "$WORK/fz.err"; fz_bad=$((fz_bad+1)); continue 2
        fi
        out=$("$WORK/fz0" 2>&1)
        [ -z "$ref" ] && ref="$out"
        if [ "$out" != "$ref" ]; then
            say "fuzz seed $seed [$flags]" "DIFFERENT ($out against $ref)"; fz_bad=$((fz_bad+1))
        fi
    done
    if ! "$FC1" "$WORK/fz.fi" -o "$WORK/fz1" > "$WORK/fz.err" 2>&1; then
        say "fuzz seed $seed [firnc1]" "FAILED"; head -3 "$WORK/fz.err"; fz_bad=$((fz_bad+1)); continue
    fi
    out=$("$WORK/fz1" 2>&1)
    [ "$out" != "$ref" ] && { say "fuzz seed $seed [firnc1]" "DIFFERENT ($out against $ref)"; fz_bad=$((fz_bad+1)); }
    if command -v qemu-aarch64 > /dev/null 2>&1; then
        if "$FIRNC" --target=aarch64-linux -o "$WORK/fza" "$WORK/fz.fi" > "$WORK/fz.err" 2>&1; then
            out=$(qemu-aarch64 "$WORK/fza" 2>&1)
            [ "$out" != "$ref" ] && { say "fuzz seed $seed [aarch64]" "DIFFERENT ($out against $ref)"; fz_bad=$((fz_bad+1)); }
        else
            say "fuzz seed $seed [aarch64]" "firnc0 FAILED"; head -3 "$WORK/fz.err"; fz_bad=$((fz_bad+1))
        fi
    fi
done
n=$((n+SEEDS))
fail=$((fail+fz_bad))
say "fuzz: $SEEDS random tuple programs (firnc1, 4 levels of firnc0, aarch64)" "$([ "$fz_bad" = 0 ] && echo OK || echo "$fz_bad DEVIATIONS")"

echo "  tuples: $n checks through both compilers, $fail deviations"
[ "$fail" -eq 0 ]
